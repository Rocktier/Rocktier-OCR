//! pdfium bindings, page rendering and the born-digital page test.
//!
//! The library comes from `resources/pdfium-runtime` when bundled, with the
//! same fallback candidates the CLI uses for development.

use anyhow::Result;
use pdfium_render::prelude::*;
use tauri::Manager;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

fn candidate_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("OCR_PDFIUM_DIR") {
        dirs.push(PathBuf::from(p));
    }
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir.join("pdfium-runtime"));
        dirs.push(dir.join("resources").join("pdfium-runtime"));
        dirs.push(dir.clone());
    }
    // Dev fallbacks: the family's PDF product ships the same runtime.
    dirs.push(PathBuf::from("../Rocktier PDF/src-tauri/resources/pdfium-runtime"));
    dirs.push(PathBuf::from("src-tauri/resources/pdfium-runtime"));
    dirs
}

/// Bind pdfium once per process - a second bind in the same process fails.
pub fn init_pdfium(app: &tauri::AppHandle) -> Result<&'static Pdfium> {
    if let Some(existing) = PDFIUM.get() {
        return Ok(existing);
    }
    for dir in candidate_dirs(app) {
        let candidate = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if !candidate.exists() {
            continue;
        }
        match Pdfium::bind_to_library(&candidate) {
            Ok(bindings) => {
                return Ok(PDFIUM.get_or_init(|| Pdfium::new(bindings)));
            }
            Err(_) => continue,
        }
    }
    Err(anyhow::anyhow!("no pdfium library found"))
}

/// A page whose text is this short is treated as a scan. Born-digital pages
/// carry hundreds of characters; scans carry none.
pub fn page_text_len(page: &PdfPage) -> usize {
    page.text().map(|t| t.chars().len()).unwrap_or(0)
}

/// Render a page at the requested DPI, matching the aspect of its MediaBox.
pub fn render_page(page: &PdfPage, dpi: f64) -> Result<image::DynamicImage> {
    let width_px = (page.width().value as f64 * dpi / 72.0).round() as i32;
    let config = PdfRenderConfig::new().set_target_width(width_px.clamp(32, 65000));
    let bitmap = page.render_with_config(&config)?;
    Ok(bitmap.as_image()?)
}

/// The models directory, resource dir first, measurement venv second.
pub fn model_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    if let Ok(p) = std::env::var("OCR_MODELS_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(dir) = app.path().resource_dir() {
        let candidate = dir.join("models");
        if candidate.join("ch_PP-OCRv4_det_infer.onnx").exists() {
            return Ok(candidate);
        }
    }
    let dev = Path::new("../.venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models");
    if dev.join("ch_PP-OCRv4_det_infer.onnx").exists() {
        return Ok(dev.to_path_buf());
    }
    Err(anyhow::anyhow!("no OCR models directory found"))
}
