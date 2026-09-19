#!/usr/bin/env python3
"""Batch OCR with PP-OCR, emitting boxes as well as text.

Runs inside the measurement venv (`.venv-ocr`), where rapidocr-onnxruntime and
its bundled ONNX models live. The model loads once for every image it is given,
which is why this is a batch tool: reloading detector and recogniser per page
dominates the runtime.

    .venv-ocr/bin/python ocr-ppocr.py <image> [image...]

One JSON array per image, NUL-separated, each element {box, text, score}. NUL
because OCR text is full of newlines and tabs and cannot be a delimiter. Boxes
are kept so the caller can put the blocks in human reading order; without them
the only ordering available is whatever the detector happened to emit.
"""

import json
import sys

from rapidocr_onnxruntime import RapidOCR


def main() -> None:
    paths = sys.argv[1:]
    if not paths:
        sys.exit("usage: ocr-ppocr.py <image> [image...]")

    engine = RapidOCR()
    pages = []
    for path in paths:
        blocks, _ = engine(path)
        pages.append(json.dumps([
            {"box": [[float(x), float(y)] for x, y in box], "text": text, "score": float(score)}
            for box, text, score in (blocks or [])
        ]))
    sys.stdout.write("\0".join(pages))


if __name__ == "__main__":
    main()
