//! PP-OCRv4 in Rust: the same three ONNX models the measurement venv runs,
//! driven by ONNX Runtime directly. The Python engine stays the reference;
//! this crate is the product.

pub mod det;

pub use det::{boxes_from_bitmap, detect, preprocess, DetParams, TextBox};
