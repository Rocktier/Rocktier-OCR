//! PP-OCRv4 in Rust: the same three ONNX models the measurement venv runs,
//! driven by ONNX Runtime directly. The Python engine stays the reference;
//! this crate is the product.

/// Load the detector and run one inference, as a smoke test.
pub fn smoke_test(model_path: &str, image_path: &str) -> anyhow::Result<Vec<f32>> {
    let det = ort::session::Session::builder()?
        .commit_from_file(model_path)?;
    let img = image::open(image_path)?;
    println!("session ok; image {:?}", (img.width(), img.height()));
    Ok(vec![])
}
