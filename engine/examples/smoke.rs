//! Smoke: the full detector - preprocess, inference, DB post-process.

use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("..");
    let model = root.join(".venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models/ch_PP-OCRv4_det_infer.onnx");
    let img_path = root.join("corpus-zh/库存汇总表-加盟商.jpg");

    let mut det = ort::session::Session::builder()?.commit_from_file(&model)?;
    let img = image::open(&img_path)?.to_rgb8();
    let t0 = std::time::Instant::now();
    let boxes = ocr_engine::det::detect(&mut det, &img, &Default::default())?;
    println!("检测 {} 个框，耗时 {:?}", boxes.len(), t0.elapsed());
    for b in boxes.iter().take(6) {
        let (x, y) = b.box4[0];
        println!("  score={:.3}  @({x},{y})  {:?}", b.score, b.box4);
    }
    // 供双引擎对照：写入 JSON
    let out = serde_json::json!(boxes.iter().map(|b| {
        serde_json::json!({"box": b.box4, "score": b.score})
    }).collect::<Vec<_>>());
    std::fs::write("/tmp/rust-det-boxes.json", serde_json::to_vec(&out)?)?;
    Ok(())
}
