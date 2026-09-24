//! End-to-end searchable PDF: a PDF goes in, a searchable PDF comes out.
//!
//! Pages that already carry text are left untouched - a born-digital page
//! needs nothing and layering it again would only stack. Pages without text
//! are rasterised through pdfium, recognised, and layered through the writer
//! library, with `replace` stripping any dead text the scan was hiding.

use anyhow::Result;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};

const DPI: f64 = 200.0;
/// A page with fewer characters than this counts as a scan.
const TEXT_CHARS_THRESHOLD: usize = 10;

fn pdfium_candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("OCR_PDFIUM_DIR") {
        dirs.push(PathBuf::from(p));
    }
    // The family's PDF product ships the same runtime; borrow it in dev.
    dirs.push(PathBuf::from("../Rocktier PDF/src-tauri/resources/pdfium-runtime"));
    dirs.push(PathBuf::from("resources/pdfium-runtime"));
    dirs.push(PathBuf::from("src-tauri/resources/pdfium-runtime"));
    dirs
}

fn bind_pdfium() -> Result<Pdfium> {
    for dir in pdfium_candidates() {
        let candidate = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if candidate.exists() {
            let bindings = Pdfium::bind_to_library(&candidate)
                .map_err(|e| anyhow::anyhow!("found {} but bind failed: {e}", candidate.display()))?;
            eprintln!("  pdfium: {}", candidate.display());
            return Ok(Pdfium::new(bindings));
        }
    }
    Err(anyhow::anyhow!(
        "no pdfium library found; set OCR_PDFIUM_DIR to the directory holding libpdfium.dylib"
    ))
}

fn model_dir() -> PathBuf {
    if let Ok(p) = std::env::var("OCR_MODELS_DIR") {
        return PathBuf::from(p);
    }
    let candidates = [
        PathBuf::from(".venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models"),
        PathBuf::from("../.venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models"),
    ];
    candidates.iter().find(|p| p.exists()).cloned().unwrap_or_else(|| candidates[0].clone())
}

/// The engine is built once per process: the models cost seconds to load and
/// the cost must be paid once per document, not once per page.
fn build_pipeline() -> Result<ocr_engine::Pipeline> {
    let dir = model_dir();
    let m = |n: &str| dir.join(n);
    Ok(ocr_engine::Pipeline::load(
        &m("ch_PP-OCRv4_det_infer.onnx"),
        &m("ch_ppocr_mobile_v2.0_cls_infer.onnx"),
        &m("ch_PP-OCRv4_rec_infer.onnx"),
    )?)
}

/// Blocks for the writer, in its own JSON dialect: the quad in raster pixels,
/// the text, and the per-word boxes the recogniser measured.
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        anyhow::bail!("usage: ocr-cli <input.pdf> <out.pdf> [--dpi 200]");
    }
    let input = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let dpi = args
        .iter()
        .position(|a| a == "--dpi")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse::<f64>())
        .transpose()?
        .unwrap_or(DPI);

    let mut pipe = build_pipeline()?;
    // An image has no document to preserve: it becomes a one-page searchable
    // PDF with the picture as its page.
    if !input.to_string_lossy().to_lowercase().ends_with(".pdf") {
        let img = image::open(input)?;
        eprintln!("  图片 {}x{}，识别中…", img.width(), img.height());
        let lines = pipe.run(&img)?;
        eprintln!("  识别 {} 行", lines.len());
        let tmp = std::env::temp_dir().join("ocr-cli-pages.json");
        std::fs::write(&tmp, serde_json::to_vec(&blocks_json(&lines))?)?;
        // The writer embeds the page as JPEG, so anything else is transcoded.
        let jpeg_path = if input.to_string_lossy().to_lowercase().ends_with(".jpg")
            || input.to_string_lossy().to_lowercase().ends_with(".jpeg")
        {
            input.to_path_buf()
        } else {
            let dst = std::env::temp_dir().join("ocr-cli-page.jpg");
            img.write_to(&mut std::fs::File::create(&dst)?, image::ImageFormat::Jpeg)?;
            dst
        };
        write_searchable::write_from_image(
            jpeg_path.to_str().unwrap(),
            tmp.to_str().unwrap(),
            out.to_str().unwrap(),
            img.width() as i64,
            img.height() as i64,
            dpi,
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        eprintln!("  ✅ 输出（图片 → 一页可搜索 PDF）: {out:?}");
        return Ok(());
    }

    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(input, None)?;
    let page_count = doc.pages().len() as usize;
    eprintln!("  {input:?}: {page_count} 页 @ {dpi} dpi");

    let mut pages: Vec<serde_json::Value> = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let page = doc.pages().get(i as i32)?;
        // Born-digital pages already carry their text; only a page with (almost)
        // none is a scan that needs a layer.
        let text_chars = page.text()?.chars().len();
        if text_chars >= TEXT_CHARS_THRESHOLD {
            eprintln!("  页 {}: 自带文字（{text_chars} 字符），跳过", i + 1);
            pages.push(serde_json::json!([]));
            continue;
        }
        let width_px = (page.width().value as f64 * dpi / 72.0).round() as i32;
        let config = PdfRenderConfig::new().set_target_width((width_px.max(32)).min(65000));
        let bitmap = page.render_with_config(&config)?;
        let img = bitmap.as_image()?;
        eprintln!("  页 {}: 扫描页（无文字层），栅格 {}px，识别中…", i + 1, img.width());
        let lines = pipe.run(&img)?;
        eprintln!("  页 {}: {} 行文本", i + 1, lines.len());
        pages.push(blocks_json(&lines));
    }

    let tmp = std::env::temp_dir().join("ocr-cli-pages.json");
    std::fs::write(&tmp, serde_json::to_vec(&pages)?)?;
    write_searchable::layer_on_existing(
        input.to_str().unwrap(),
        tmp.to_str().unwrap(),
        out.to_str().unwrap(),
        dpi,
        true,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    eprintln!("  ✅ 输出: {out:?}");
    Ok(())
}
