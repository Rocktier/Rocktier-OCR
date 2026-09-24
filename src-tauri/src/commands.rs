//! The two commands the UI talks to: process a document, cancel it.

use crate::pdf;
use ocr_engine::Pipeline;
use serde::Serialize;
use std::path::{Path, PathBuf};
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
    /// True when every page already carried a text layer: there was nothing
    /// for the recogniser to do, and the UI should say so in plain words.
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

/// Process a document: rasterise, recognise, layer, save. Emits
/// `ocr-progress` per page; a page already carrying text is left untouched.
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
        let pdfium = pdf::init_pdfium(&app_for_pdf).map_err(|e| e.to_string())?;
        let doc = pdfium
            .load_pdf_from_file(Path::new(&input), None)
            .map_err(|e| e.to_string())?;
        let total = doc.pages().len() as usize;
        let mut pages: Vec<serde_json::Value> = Vec::with_capacity(total);
        let (mut ocr_pages, mut skipped, mut line_count) = (0usize, 0usize, 0usize);

        for i in 0..total {
            if cancel.load(Ordering::SeqCst) {
                return Err("cancelled".into());
            }
            let _ = app.emit("ocr-progress", Progress { page: i + 1, total, phase: "detect".into() });
            let page = doc.pages().get(i as i32).map_err(|e| e.to_string())?;
            let text_chars = pdf::page_text_len(&page);
            if text_chars >= 10 {
                skipped += 1;
                pages.push(serde_json::json!([]));
                let _ = app.emit(
                    "ocr-progress",
                    Progress { page: i + 1, total, phase: format!("skipped ({text_chars} chars)") },
                );
                continue;
            }
            let _ = app.emit("ocr-progress", Progress { page: i + 1, total, phase: "render".into() });
            let img = pdf::render_page(&page, dpi).map_err(|e| e.to_string())?;
            let _ = app.emit("ocr-progress", Progress { page: i + 1, total, phase: "recognise".into() });
            let lines = with_pipeline(&pipeline, &app_for_pdf, |pipe| pipe.run(&img).map_err(|e| e.to_string()))?;
            line_count += lines.len();
            ocr_pages += 1;
            pages.push(blocks_json(&lines));
        }

        let _ = app.emit("ocr-progress", Progress { page: total, total, phase: "write".into() });
        let tmp = std::env::temp_dir().join("rocktier-ocr-pages.json");
        std::fs::write(&tmp, serde_json::to_vec(&pages).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        write_searchable::layer_on_existing(&input, tmp.to_str().unwrap(), &output, dpi, true)
            .map_err(|e| e.to_string())?;

        Ok(Summary {
            pages_total: total,
            pages_ocr: ocr_pages,
            pages_skipped: skipped,
            lines: line_count,
            already_searchable: ocr_pages == 0 && skipped > 0,
            output: output.clone(),
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

/// Resolve a path for the frontend (dialog gives absolute paths already; this
/// exists so the UI can show one without another plugin).
pub fn out_path_hint(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}-searchable.pdf"))
}
