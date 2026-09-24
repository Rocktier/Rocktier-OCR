#!/bin/bash
# 把模型与 pdfium 运行时复制进 src-tauri/resources（不进 git，见 .gitignore）。
set -e
cd "$(dirname "$0")/.."
MODELS_SRC="${OCR_MODELS_SRC:-../.venv-ocr/lib/python3.12/site-packages/rapidocr_onnxruntime/models}"
PDFIUM_SRC="${OCR_PDFIUM_SRC:-../Rocktier PDF/src-tauri/resources/pdfium-runtime}"
mkdir -p src-tauri/resources/models src-tauri/resources/pdfium-runtime
cp "$MODELS_SRC"/*.onnx src-tauri/resources/models/
cp "$PDFIUM_SRC"/libpdfium.* src-tauri/resources/pdfium-runtime/ 2>/dev/null || cp "$PDFIUM_SRC"/*pdfium* src-tauri/resources/pdfium-runtime/
echo "resources 已同步"
