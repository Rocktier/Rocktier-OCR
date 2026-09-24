//! Smoke: the whole pipeline - det, cls, rec - against the Python result.

use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("..");
    let m = |n: &str| root.join(format!(".venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models/{n}"));
    let mut pipe = ocr_engine::Pipeline::load(
        &m("ch_PP-OCRv4_det_infer.onnx"),
        &m("ch_ppocr_mobile_v2.0_cls_infer.onnx"),
        &m("ch_PP-OCRv4_rec_infer.onnx"),
    )?;
    let img = image::open(root.join("corpus-zh/库存汇总表-加盟商.jpg"))?;
    let t0 = std::time::Instant::now();
    let lines = pipe.run(&img)?;
    println!("Rust 全流水线: {} 行, 耗时 {:?}", lines.len(), t0.elapsed());

    let out: Vec<_> = lines.iter().map(|l| serde_json::json!({
        "box": l.box4, "text": l.text, "score": l.score, "words": l.words
    })).collect();
    std::fs::write("/tmp/rust-pipeline.json", serde_json::to_vec(&out)?)?;
    for l in lines.iter().take(3) {
        println!("  {:.2} 「{}」 词数 {}", l.score, l.text, l.words.len());
        for (wt, wb) in l.words.iter().take(3) {
            println!("     「{}」 @({},{})", wt, wb[0].0, wb[0].1);
        }
    }
    Ok(())
}
