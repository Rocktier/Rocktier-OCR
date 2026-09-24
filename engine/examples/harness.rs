//! Double-engine harness: run the Rust pipeline on given images, dump JSON.

use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("..");
    let m = |n: &str| root.join(format!(".venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models/{n}"));
    let mut pipe = ocr_engine::Pipeline::load(
        &m("ch_PP-OCRv4_det_infer.onnx"),
        &m("ch_ppocr_mobile_v2.0_cls_infer.onnx"),
        &m("ch_PP-OCRv4_rec_infer.onnx"),
    )?;
    std::fs::create_dir_all("/tmp/rust-harness")?;
    for arg in std::env::args().skip(1) {
        let p = Path::new(&arg);
        let stem = p.file_stem().unwrap().to_string_lossy();
        let img = image::open(p)?;
        let lines = pipe.run(&img)?;
        let out: Vec<_> = lines.iter().map(|l| serde_json::json!({
            "box": l.box4, "text": l.text, "score": l.score, "words": l.words
        })).collect();
        let dst = Path::new("/tmp/rust-harness").join(format!("{stem}.json"));
        std::fs::write(&dst, serde_json::to_vec(&out)?)?;
        eprintln!("  {} → {} 行", stem, lines.len());
    }
    Ok(())
}
