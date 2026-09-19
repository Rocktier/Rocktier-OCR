#!/usr/bin/env python3
"""Batch OCR with PP-OCR, for baseline.py.

Runs inside the measurement venv (`.venv-ocr`), where rapidocr-onnxruntime and
its bundled ONNX models live. Loads the model once for every image it is given,
which is why this is a batch tool rather than a per-page call: reloading the
detector and recogniser for each page dominates the runtime.

    .venv-ocr/bin/python ocr-ppocr.py <image> [image...]

Texts are printed NUL-separated, one per input image, in the same order as the
arguments. NUL, not newline: OCR output contains newlines and tabs, and the
caller has to be able to split the results reliably.
"""

import sys

from rapidocr_onnxruntime import RapidOCR


def main() -> None:
    paths = sys.argv[1:]
    if not paths:
        sys.exit("usage: ocr-ppocr.py <image> [image...]")

    engine = RapidOCR()
    results = []
    for path in paths:
        blocks, _ = engine(path)
        # Reading order is already top-to-bottom, left-to-right.
        results.append("\n".join(text for _, text, _ in (blocks or [])))

    sys.stdout.write("\0".join(results))


if __name__ == "__main__":
    main()
