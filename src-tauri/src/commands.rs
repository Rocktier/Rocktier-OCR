//! The commands the UI talks to.
//!
//! Recognition and output are separate steps, because they are separate
//! questions: "what does this page say" is answered once, and "what shall I
//! do with the words" can then be answered three times over - a searchable
//! PDF, a TXT file, or the clipboard - without paying for recognition again.
//! So converting a document fills the state with a finished job, and each
//! output command works from that job.
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

/// A document that has been read: what each page says, and where it came from.
#[derive(Clone)]
pub struct Job {
    pub input: String,
    pub is_pdf: bool,
    pub dpi: f64,
    /// Recognised blocks per page, empty for pages that already carried text.
    pub blocks: Vec<serde_json::Value>,
    /// Text per page: the document's own words where it had them, the
    /// recognised lines where it did not.
    pub page_text: Vec<String>,
}

pub struct AppState {
    pub cancel: Arc<AtomicBool>,
    pub pipeline: Arc<Mutex<Option<Pipeline>>>,
    /// The last finished conversion, which every output is drawn from.
    pub last: Arc<Mutex<Option<Job>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            pipeline: Arc::new(Mutex::new(None)),
            last: Arc::new(Mutex::new(None)),
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
    /// "pdf" or "image" - the frontend says what it was given.
    kind: String,
    /// Every page already carried a text layer: there was nothing to do.
    already_searchable: bool,
}

#[derive(Serialize)]
pub struct TextResult {
    text: String,
    pages: usize,
    lines: usize,
    chars: usize,
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

fn page_separator(n: usize) -> String {
    format!("\n\n--- Page {n} ---\n\n")
}

/// Read a document and remember what it said. Produces nothing by itself -
/// the three output commands do that, from what this one leaves behind.
#[tauri::command(async)]
pub async fn ocr_process(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    dpi: Option<f64>,
) -> Result<Summary, String> {
    let dpi = dpi.unwrap_or(200.0);
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    let pipeline = state.pipeline.clone();
    let last = state.last.clone();

    let app_for_pdf = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Summary, String> {
        let is_pdf = input.to_lowercase().ends_with(".pdf");
        let mut blocks: Vec<serde_json::Value> = Vec::new();
        let mut page_text: Vec<String> = Vec::new();
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
                    // Already readable: keep the document's own words rather
                    // than asking the recogniser to guess at them again.
                    skipped += 1;
                    let own = page.text().map_err(|e| e.to_string())?.all();
                    blocks.push(serde_json::json!([]));
                    page_text.push(own);
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
                page_text.push(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
                blocks.push(blocks_json(&lines));
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
            page_text.push(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
            blocks.push(blocks_json(&lines));
        }

        let _ = app.emit("ocr-progress", Progress { page: total, total, phase: "done".into() });
        *last.lock().map_err(|e| e.to_string())? = Some(Job {
            input: input.clone(),
            is_pdf,
            dpi,
            blocks,
            page_text,
        });

        Ok(Summary {
            pages_total: total,
            pages_ocr: ocr_pages,
            pages_skipped: skipped,
            lines: line_count,
            kind: if is_pdf { "pdf".to_string() } else { "image".to_string() },
            already_searchable: ocr_pages == 0 && skipped > 0,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write the converted document as a searchable PDF.
#[tauri::command(async)]
pub async fn ocr_export_pdf(
    state: State<'_, AppState>,
    output: String,
) -> Result<serde_json::Value, String> {
    let job = state
        .last
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "还没有转换完成的文档。".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let tmp = std::env::temp_dir().join("rocktier-ocr-pages.json");
        // A PDF takes one entry per page; the single-image path takes the flat
        // block array, which is the same shape that mode was written for.
        let payload = if job.is_pdf {
            serde_json::to_vec(&job.blocks)
        } else {
            serde_json::to_vec(job.blocks.first().unwrap_or(&serde_json::json!([])))
        }
        .map_err(|e| e.to_string())?;
        std::fs::write(&tmp, payload).map_err(|e| e.to_string())?;

        if job.is_pdf {
            write_searchable::layer_on_existing(&job.input, tmp.to_str().unwrap(), &output, job.dpi, true)
                .map_err(|e| e.to_string())?;
        } else {
            let img = image::open(&job.input).map_err(|e| e.to_string())?;
            let (w, h) = (img.width() as i64, img.height() as i64);
            // The page is embedded as JPEG, so any other format is transcoded
            // first - a PNG's bytes declared as DCTDecode is a broken picture.
            let page = if job.input.to_lowercase().ends_with(".jpg")
                || job.input.to_lowercase().ends_with(".jpeg")
            {
                job.input.clone()
            } else {
                let dst = std::env::temp_dir().join("rocktier-ocr-page.jpg");
                img.write_to(
                    &mut std::fs::File::create(&dst).map_err(|e| e.to_string())?,
                    image::ImageFormat::Jpeg,
                )
                .map_err(|e| e.to_string())?;
                dst.to_string_lossy().to_string()
            };
            write_searchable::write_from_image(&page, tmp.to_str().unwrap(), &output, w, h, job.dpi)
                .map_err(|e| e.to_string())?;
        }

        Ok(serde_json::json!({ "output": output, "pages": job.page_text.len() }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The converted document's text, written to a TXT file when a path is given.
#[tauri::command(async)]
pub async fn ocr_export_txt(
    state: State<'_, AppState>,
    output: Option<String>,
) -> Result<TextResult, String> {
    let job = state
        .last
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "还没有转换完成的文档。".to_string())?;

    tauri::async_runtime::spawn_blocking(move || -> Result<TextResult, String> {
        let mut text = String::new();
        let mut lines = 0usize;
        for (i, page) in job.page_text.iter().enumerate() {
            lines += page.lines().count().max(1);
            if job.is_pdf {
                text.push_str(&page_separator(i + 1));
            }
            text.push_str(page.trim());
        }
        if let Some(path) = output {
            std::fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(TextResult {
            chars: text.chars().count(),
            text,
            pages: job.page_text.len(),
            lines,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ask the running job to stop at the next page boundary.
#[tauri::command]
pub fn ocr_cancel(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}
