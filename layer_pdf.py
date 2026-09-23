#!/usr/bin/env python3
"""Put a text layer on every page of an existing PDF, and report what survived.

    .venv-ocr/bin/python layer_pdf.py <pdf> [--dpi 200] [--max-pages 5]

This is the multi-page path. The one-page writer builds a PDF around a rasterised
page, which throws away every other page of the document - measured on the corpus,
378 of 398 pages were lost. This opens the original instead and appends a text
layer to each page, so pages, images, metadata and bookmarks were never discarded
and cannot go missing.
"""

import argparse
import json
import pathlib
import subprocess
import sys

import baseline

ROOT = pathlib.Path(__file__).resolve().parent
WRITER = ROOT / "writer/target/release/write-searchable"


def info(pdf: pathlib.Path, key: str) -> str:
    out = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith(key + ":"):
            return line.split(":", 1)[1].strip()
    return ""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf")
    ap.add_argument("--dpi", type=int, default=200)
    ap.add_argument("--max-pages", type=int, default=0, help="0 = every page")
    ap.add_argument("--replace", action="store_true",
                    help="take out the text each page already had")
    a = ap.parse_args()

    pdf = pathlib.Path(a.pdf)
    if not pdf.exists():
        sys.exit(f"no such file: {pdf}")

    work = pathlib.Path("/tmp/ocr-layer")
    work.mkdir(exist_ok=True)
    for stale in work.glob("*"):
        stale.unlink()

    subprocess.run(
        ["pdftoppm", "-r", str(a.dpi), "-png", str(pdf), str(work / "p")],
        check=True, capture_output=True,
    )
    images = sorted(work.glob("p-*.png"))
    if a.max_pages:
        images = images[: a.max_pages]
    if not images:
        sys.exit("nothing rasterised")
    print(f"  {pdf.name}  {info(pdf, 'Pages')} 页，本次处理 {len(images)} 页")

    pages = baseline.ocr_all(images, "eng", "ppocr")
    pages_json = work / "pages.json"
    pages_json.write_text(json.dumps(pages))

    out = work / "layered.pdf"
    r = subprocess.run(
        [str(WRITER), "--pdf", str(pdf), str(pages_json), str(out), str(a.dpi)]
        + (["replace"] if a.replace else []),
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        sys.exit(f"writer failed: {r.stderr[:400]}")

    ocr_words = sum(
        len(b["text"].split()) for p in pages for b in (p or [])
    )
    text = subprocess.run(["pdftotext", str(out), "-"], capture_output=True, text=True).stdout

    print()
    print(f"  页数        输入 {info(pdf, 'Pages')}  →  输出 {info(out, 'Pages')}")
    print(f"  标题        输入 {info(pdf, 'Title') or '(无)'}  →  输出 {info(out, 'Title') or '(无)'}")
    print(f"  页面尺寸    输入 {info(pdf, 'Page size')}  →  输出 {info(out, 'Page size')}")
    print(f"  文件体积    输入 {pdf.stat().st_size} B  →  输出 {out.stat().st_size} B")
    print()
    print(f"  OCR 词数    {ocr_words}   （{'替换模式' if a.replace else '追加模式'}）")
    print(f"  抽回的词    {len(text.split())}")

    side = pathlib.Path(str(out) + ".boxes.json")
    if side.exists():
        boxes = json.loads(side.read_text())
        pages_covered = sorted({b.get("page", 0) for b in boxes})
        print(f"  sidecar     {len(boxes)} 个词，覆盖 {len(pages_covered)} 页")

    # The images have to still be there - the layer is added, not swapped in.
    def images_in(p: pathlib.Path) -> int:
        o = subprocess.run(["pdfimages", "-list", str(p)], capture_output=True, text=True).stdout
        return sum(1 for line in o.splitlines()[2:] if line.strip())

    print(f"  每页图像数  输入 {images_in(pdf)}  →  输出 {images_in(out)}")


if __name__ == "__main__":
    main()
