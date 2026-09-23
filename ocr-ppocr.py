#!/usr/bin/env python3
"""Batch OCR with PP-OCR, emitting boxes as well as text.

Runs inside the measurement venv (`.venv-ocr`), where rapidocr-onnxruntime and
its bundled ONNX models live. The model loads once for every image it is given,
which is why this is a batch tool: reloading detector and recogniser per page
dominates the runtime.

    .venv-ocr/bin/python ocr-ppocr.py <image> [image...]

One JSON array per image, NUL-separated, each element {box, text, score, words}.
NUL because OCR text is full of newlines and tabs and cannot be a delimiter.

`words` is the addition that matters. The recogniser can return a box per
character, spaces included; grouping those gives every word its real width and
its real gap. Without it the write side had to spread words across the line's box
by estimated length, which is where words came back fused to their neighbours.
"""

import json
import sys

from rapidocr_onnxruntime import RapidOCR


def group_words(char_boxes, chars):
    """Turn per-character boxes into per-word boxes, splitting on the spaces the
    recogniser emits.

    A character that has no box, or a box that is degenerate, is skipped rather
    than trusted: the point of taking boxes from the engine is to stop guessing.
    """
    words = []
    run = []
    for box, ch in zip(char_boxes, chars):
        if ch == " ":
            if run:
                words.append(_merge(run))
                run = []
            continue
        run.append((box, ch))
    if run:
        words.append(_merge(run))
    return words


def _merge(run):
    xs = [x for box, _ in run for x, y in box]
    ys = [y for box, _ in run for x, y in box]
    return {
        "box": [[min(xs), min(ys)], [max(xs), min(ys)],
                [max(xs), max(ys)], [min(xs), max(ys)]],
        "text": "".join(ch for _, ch in run),
    }


def main() -> None:
    paths = sys.argv[1:]
    if not paths:
        sys.exit("usage: ocr-ppocr.py <image> [image...]")

    engine = RapidOCR()
    pages = []
    for path in paths:
        blocks, _ = engine(path, return_word_box=True)
        out = []
        for item in blocks or []:
            entry = {
                "box": [[float(x), float(y)] for x, y in item[0]],
                "text": item[1],
                "score": float(item[2]),
            }
            # With return_word_box the result carries a box per character alongside
            # the characters themselves. Only add the field when both are present and
            # the same length, so a caller can never act on a partial mapping.
            if len(item) >= 5 and item[3] and item[4] and len(item[3]) == len(item[4]):
                words = group_words(item[3], item[4])
                if words:
                    entry["words"] = words
            out.append(entry)
        pages.append(json.dumps(out))
    sys.stdout.write("\0".join(pages))


if __name__ == "__main__":
    main()
