//! The two commands the UI talks to: process a document, cancel it.
//!
//! Two inputs are accepted. A PDF keeps its own identity and gains a text
//! layer, page by page, skipping pages that already have text. An image has no
//! identity to keep, so it becomes a one-page searchable PDF with the picture
//! as its page - the same idea as scanning a sheet.

use crate::pdf;
use ocr_engine::Pipeline;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub cancel: Arc<AtomicBool>,
    pub pipeline: Arc<Mutex<Option<Pipeline>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            pipeline: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    page: usize,
    total: usize,
    phase: String,
}

#[derive(Serialize)]
pub struct Summary {
    pages_total: usize,
    pages_ocr: usize,
    pages_skipped: usize,
    lines: usize,
    output: String,
    /// "pdf" or "image" - the frontend says what it made from each.
    kind: String,
    /// Every page already carried a text layer: there was nothing to do.
    already_searchable: bool,
}

fn with_pipeline<F, T>(
    pipeline: &Arc<Mutex<Option<Pipeline>>>,
    app: &AppHandle,
    f: F,
) -> Result<T, String>
where
    F: FnOnce(&mut Pipeline) -> Result<T, String>,
{
    let dir = pdf::model_dir(app).map_err(|e| e.to_string())?;
    let m = |n: &str| dir.join(n);
    let mut guard = pipeline.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(
            Pipeline::load(
                &m("ch_PP-OCRv4_det_infer.onnx"),
                &m("ch_ppocr_mobile_v2.0_cls_infer.onnx"),
                &m("ch_PP-OCRv4_rec_infer.onnx"),
            )
            .map_err(|e| e.to_string())?,
        );
    }
    f(guard.as_mut().unwrap())
}

/// Blocks for the writer, in its own JSON dialect.
fn blocks_json(lines: &[ocr_engine::OcrLine]) -> serde_json::Value {
    serde_json::json!(lines
        .iter()
        .map(|l| {
            serde_json::json!({
                "box": l.box4.iter().map(|p| [p.0, p.1]).collect::<Vec<_>>(),
                "text": l.text,
                "score": l.score,
                "words": l.words.iter().map(|(t, q)| {
                    serde_json::json!({
                        "text": t,
                        "box": q.iter().map(|p| [p.0, p.1]).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>())
}

#[tauri::command(async)]
pub async fn ocr_process(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: String,
    dpi: Option<f64>,
) -> Result<Summary, String> {
    let dpi = dpi.unwrap_or(200.0);
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    let pipeline = state.pipeline.clone();

    let app_for_pdf = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Summary, String> {
        let is_pdf = input.to_lowercase().ends_with(".pdf");
        let mut pages: Vec<serde_json::Value> = Vec::new();
        let (mut ocr_pages, mut skipped, mut line_count) = (0usize, 0usize, 0usize);
        let total: usize;

        if is_pdf {
            let pdfium = pdf::init_pdfium(&app_for_pdf).map_err(|e| e.to_string())?;
            let doc = pdfium
                .load_pdf_from_file(Path::new(&input), None)
                .map_err(|e| e.to_string())?;
            total = doc.pages().len() as usize;
            for i in 0..total {
                if cancel.load(Ordering::SeqCst) {
                    return Err("cancelled".into());
                }
                let _ = app.emit(
                    "ocr-progress",
                    Progress { page: i + 1, total, phase: "detect".into() },
                );
                let page = doc.pages().get(i as i32).map_err(|e| e.to_string())?;
                let text_chars = pdf::page_text_len(&page);
                if text_chars >= 10 {
                    skipped += 1;
                    pages.push(serde_json::json!([]));
                    let _ = app.emit(
                        "ocr-progress",
                        Progress {
                            page: i + 1,
                            total,
                            phase: format!("skipped ({text_chars} chars)"),
                        },
                    );
                    continue;
                }
                let _ = app.emit("ocr-progress", Progress { page: i + 1, total, phase: "render".into() });
                let img = pdf::render_page(&page, dpi).map_err(|e| e.to_string())?;
                let _ = app.emit("ocr-progress", Progress { page: i + 1, total, phase: "recognise".into() });
                let lines = with_pipeline(&pipeline, &app_for_pdf, |pipe| {
                    pipe.run(&img).map_err(|e| e.to_string())
                })?;
                line_count += lines.len();
                ocr_pages += 1;
                pages.push(blocks_json(&lines));
            }
        } else {
            total = 1;
            let _ = app.emit("ocr-progress", Progress { page: 1, total, phase: "recognise".into() });
            let img = image::open(&input).map_err(|e| format!("无法打开图片：{e}"))?;
            let lines = with_pipeline(&pipeline, &app_for_pdf, |pipe| {
                pipe.run(&img).map_err(|e| e.to_string())
            })?;
            line_count += lines.len();
            ocr_pages += 1;
            pages.push(blocks_json(&lines));
        }

        let _ = app.emit("ocr-progress", Progress { page: total, total, phase: "write".into() });
        let tmp = std::env::temp_dir().join("rocktier-ocr-pages.json");
        // A PDF takes one entry per page; the single-image path takes the flat
        // block array, which is the same shape that mode was written for.
        let payload = if is_pdf {
            serde_json::to_vec(&pages)
        } else {
            serde_json::to_vec(pages.first().unwrap_or(&serde_json::json!([])))
        }
        .map_err(|e| e.to_string())?;
        std::fs::write(&tmp, payload).map_err(|e| e.to_string())?;

        if is_pdf {
            write_searchable::layer_on_existing(&input, tmp.to_str().unwrap(), &output, dpi, true)
                .map_err(|e| e.to_string())?;
        } else {
            let img = image::open(&input).map_err(|e| e.to_string())?;
            let (w, h) = (img.width() as i64, img.height() as i64);
            // The page is embedded as JPEG, so any other format is transcoded
            // first - a PNG's bytes declared as DCTDecode is a broken picture.
            let page = if input.to_lowercase().ends_with(".jpg") || input.to_lowercase().ends_with(".jpeg") {
                input.clone()
            } else {
                let dst = std::env::temp_dir().join("rocktier-ocr-page.jpg");
                img.write_to(&mut std::fs::File::create(&dst).map_err(|e| e.to_string())?, image::ImageFormat::Jpeg)
                    .map_err(|e| e.to_string())?;
                dst.to_string_lossy().to_string()
            };
            write_searchable::write_from_image(&page, tmp.to_str().unwrap(), &output, w, h, dpi)
                .map_err(|e| e.to_string())?;
        }

        let kind = if is_pdf { "pdf".to_string() } else { "image".to_string() };
        Ok(Summary {
            pages_total: total,
            pages_ocr: ocr_pages,
            pages_skipped: skipped,
            lines: line_count,
            already_searchable: ocr_pages == 0 && skipped > 0,
            output: output.clone(),
            kind,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The text of a document, page by page. Pages that already carry text give
/// up theirs directly - re-recognising a born-digital page would only add
/// noise - and pages that do not are recognised first. Used by the TXT export
/// and by "copy everything".
#[derive(Serialize)]
pub struct TextResult {
    text: String,
    pages: usize,
    lines: usize,
    chars: usize,
}

fn page_separator(n: usize) -> String {
    format!("\n\n--- Page {n} ---\n\n")
}

#[tauri::command(async)]
pub async fn extract_text(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    output: Option<String>,
) -> Result<TextResult, String> {
    let pipeline = state.pipeline.clone();
    let app_for_pdf = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<TextResult, String> {
        let is_pdf = input.to_lowercase().ends_with(".pdf");
        let mut text = String::new();
        let (mut pages, mut lines) = (0usize, 0usize);

        if is_pdf {
            let pdfium = pdf::init_pdfium(&app_for_pdf).map_err(|e| e.to_string())?;
            let doc = pdfium
                .load_pdf_from_file(Path::new(&input), None)
                .map_err(|e| e.to_string())?;
            let total = doc.pages().len() as usize;
            for i in 0..total {
                let _ = app.emit(
                    "ocr-progress",
                    Progress { page: i + 1, total, phase: "text".into() },
                );
                let page = doc.pages().get(i as i32).map_err(|e| e.to_string())?;
                let page_text = if pdf::page_text_len(&page) >= 10 {
                    // Born-digital: the document already knows its words.
                    page.text().map_err(|e| e.to_string())?.all()
                } else {
                    let img = pdf::render_page(&page, 200.0).map_err(|e| e.to_string())?;
                    let found = with_pipeline(&pipeline, &app_for_pdf, |pipe| {
                        pipe.run(&img).map_err(|e| e.to_string())
                    })?;
                    found
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                lines += page_text.lines().count().max(0);
                pages += 1;
                text.push_str(&page_separator(i + 1));
                text.push_str(page_text.trim());
            }
        } else {
            let _ = app.emit(
                "ocr-progress",
                Progress { page: 1, total: 1, phase: "text".into() },
            );
            let img = image::open(&input).map_err(|e| format!("无法打开图片：{e}"))?;
            let found = with_pipeline(&pipeline, &app_for_pdf, |pipe| {
                pipe.run(&img).map_err(|e| e.to_string())
            })?;
            lines = found.len();
            pages = 1;
            text.push_str(&found.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
        }

        if let Some(path) = output {
            std::fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        }
        let chars = text.chars().count();
        Ok(TextResult { text, pages, lines, chars })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ask the running job to stop at the next page boundary.
#[tauri::command]
pub fn ocr_cancel(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}
