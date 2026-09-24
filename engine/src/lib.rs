//! PP-OCRv4 in Rust: the same three ONNX models the measurement venv runs,
//! driven by ONNX Runtime directly. The Python engine stays the reference;
//! this crate is the product.

pub mod cls;
pub mod det;
pub mod pipeline;
pub mod rec;
pub mod words;

pub use pipeline::{OcrLine, Pipeline};
