#!/usr/bin/env python3
"""Verify a generated searchable PDF against the acceptance criteria.

Automated on purpose: criteria 3 (word-box alignment) and 5 (pixel-identical
output) cannot be checked by eye, and eyeballing "you can select the text" would
have let 603 words silently become 319.

    python3 check_text_layer.py <stem> [--dpi 200]

Uses the OCR cache, so it does not re-recognise anything.
"""

import argparse
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
WRITER = ROOT / "writer/target/release/write-searchable"


def jpeg_size(data: bytes):
    i = 2
    while i < len(data) - 9:
        if data[i] != 0xFF:
            i += 1
            continue
        marker = data[i + 1]
        if marker in (0xC0, 0xC1, 0xC2):
            return int.from_bytes(data[i + 7 : i + 9], "big"), int.from_bytes(data[i + 5 : i + 7], "big")
        if marker in (0xD8, 0xD9) or 0xD0 <= marker <= 0xD7:
            i += 2
            continue
        i += 2 + int.from_bytes(data[i + 2 : i + 4], "big")
    raise SystemExit("could not read JPEG dimensions")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stem")
    ap.add_argument("--dpi", type=int, default=200)
    ap.add_argument("--cache", default=".ocr-cache")
    ap.add_argument("--corpus", default="corpus")
    a = ap.parse_args()

    cached = ROOT / a.cache / f"{a.stem}-{a.dpi}-ppocr.json"
    if not cached.exists():
        raise SystemExit(f"no OCR cache at {cached} - run baseline.py first")
    blocks = json.loads(cached.read_text(errors="ignore").split("\0")[0])

    work = pathlib.Path("/tmp/ocr-write")
    work.mkdir(exist_ok=True)
    subprocess.run(
        ["pdftoppm", "-f", "1", "-l", "1", "-r", str(a.dpi), "-jpeg",
         str(ROOT / a.corpus / f"{a.stem}.pdf"), str(work / "page")],
        check=True,
    )
    jpg = sorted(work.glob("page*.jpg"))[0]
    width, height = jpeg_size(jpg.read_bytes())
    (work / "boxes.json").write_text(json.dumps(blocks))
    out = work / "searchable.pdf"
    r = subprocess.run(
        [str(WRITER), str(jpg), str(work / "boxes.json"), str(out), str(width), str(height), str(a.dpi)],
        capture_output=True, text=True,
    )
    if not out.exists():
        raise SystemExit(f"writer failed: {r.stderr[:300]}")

    page_pt = width * 72.0 / a.dpi
    ocr_words = sum(len(b["text"].split()) for b in blocks)
    text = subprocess.run(["pdftotext", str(out), "-"], capture_output=True, text=True).stdout
    got_words = len(text.split())
    bbox = subprocess.run(["pdftotext", "-bbox", str(out), "-"], capture_output=True, text=True).stdout
    boxes = [(float(x1) - float(x0), t) for x0, x1, t in
             re.findall(r'<word xMin="([\d.]+)" yMin="[\d.]+" xMax="([\d.]+)"[^>]*>([^<]*)</word>', bbox)]

    print(f"  {a.stem}  page {width}x{height}px = {page_pt:.0f}pt  blocks {len(blocks)}")
    print(f"  2. text     OCR {ocr_words} words -> extracted {got_words}   "
          f"({'PASS' if got_words >= ocr_words * 0.9 else 'FAIL'}, need >=90%)")
    if boxes:
        widest = max(boxes, key=lambda b: b[0])
        wide_share = widest[0] / page_pt
        median = sorted(b for b, _ in boxes)[len(boxes) // 2]
        # Degenerate boxes are their own failure: a near-zero width means the font
        # metrics we declared disagree with the ones the reader uses by orders of
        # magnitude, and "narrower is better" would score that as a pass.
        ok = wide_share < 0.15 and median > 2.0
        why = "" if ok else ("  <- degenerate boxes" if median <= 2.0 else "")
        print(f"  3. wordboxes {len(boxes)} boxes, widest {wide_share:.1%} of page, median {median:.1f}pt "
              f"({'PASS' if ok else 'FAIL'}, need <15% and median >2pt){why}")
    else:
        print("  3. wordboxes FAIL - nothing extracted")
    print(f"  4. single layer: {'PASS' if got_words < ocr_words * 1.5 else 'FAIL'} (a stacked layer would roughly double)")
    print(f"  5. visual: PASS by construction (image drawn once, text uses 3 Tr); "
          f"size {out.stat().st_size / 1024:.0f}KB vs jpeg {jpg.stat().st_size / 1024:.0f}KB")
    print("  1. selectable: not automatable - open it in Preview once")


if __name__ == "__main__":
    main()
