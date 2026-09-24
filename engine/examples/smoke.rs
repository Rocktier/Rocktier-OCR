//! Smoke: load the detector, run one inference on a real page.

use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("..");
    let model = root.join(".venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models/ch_PP-OCRv4_det_infer.onnx");
    let img_path = root.join(".ocr-cache/../corpus-zh/库存汇总表-加盟商.jpg");

    let t0 = std::time::Instant::now();
    let mut det = ort::session::Session::builder()?.commit_from_file(&model)?;
    println!("det 模型加载: {:?}", t0.elapsed());
    for (i, input) in det.inputs().iter().enumerate() {
        println!("  输入{i}: {}", input.name());
    }
    for (i, out) in det.outputs().iter().enumerate() {
        println!("  输出{i}: {}", out.name());
    }

    // rapidocr Det 预处理：min 边 >=736 才缩放到 736，否则原样；再补齐 32 倍数
    let img = image::open(&img_path)?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let scale = if w.min(h) >= 736 { 736.0 / w.min(h) as f32 } else { 1.0 };
    let rw = (w as f32 * scale).round() as usize;
    let rh = (h as f32 * scale).round() as usize;
    let nw = (rw + 31) / 32 * 32;
    let nh = (rh + 31) / 32 * 32;
    let resized = if (rw, rh) == (w, h) {
        img
    } else {
        img.resize_exact(rw as u32, rh as u32, image::imageops::FilterType::Lanczos3)
    };
    println!("  预处理: {w}x{h} → {rw}x{rh} → pad {nw}x{nh}");

    let mut input = vec![0f32; (nw * nh * 3) as usize]; // pad 区 = 0.0 = 中性灰
    for (x, y, p) in resized.to_rgb8().enumerate_pixels() {
        let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let base = (y as usize * nw + x as usize);
        input[base] = (r / 255.0 - 0.5) / 0.5;
        input[base + nw * nh] = (g / 255.0 - 0.5) / 0.5;
        input[base + 2 * nw * nh] = (b / 255.0 - 0.5) / 0.5;
    }
    let shape = [1i64, 3, nh as i64, nw as i64];
    let x = ort::value::Tensor::from_array((shape, input))?;
    let t1 = std::time::Instant::now();
    let outs = det.run(ort::inputs![x])?;
    println!("  推理: {:?}", t1.elapsed());
    let (shape, data) = outs[0].try_extract_tensor::<f32>()?;
    println!("  输出形状: {:?}  元素数: {}", shape, data.len());
    let prob_min = data.iter().cloned().fold(f32::MAX, f32::min);
    let prob_max = data.iter().cloned().fold(f32::MIN, f32::max);
    println!("  概率图范围: [{prob_min:.3}, {prob_max:.3}]  (>0.3 的像素: {})", data.iter().filter(|&&p| p > 0.3).count());
    Ok(())
}
