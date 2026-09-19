#!/usr/bin/env python3
"""OCR accuracy baseline.

The OCR project lives or dies on accuracy, so it is measured before any UI is
written (see the 2026-09-17 initiation record in progress.md). This script is
that measurement, deliberately built on tools already on the machine so it adds
no dependency: pdftotext / pdftoppm (poppler) to get ground truth and rasterise,
tesseract to recognise, difflib to score. Pure stdlib otherwise.

    scripts/ocr-baseline.py <pdf-or-dir> [more...] [--dpi 300] [--lang eng]

Ground truth is the PDF's own text layer, thrown away from the image before
recognition — that is what makes the comparison honest at zero cost.

Reported per page and averaged: character accuracy after normalising whitespace.
The number to watch is the mean; individual near-empty pages are noise.
"""

import argparse
import collections
import difflib
import json
import pathlib
import re
import subprocess
import sys
import tempfile

import reading_order

TOOLS = ("pdftotext", "pdftoppm", "tesseract")


def need(tool: str) -> None:
    if subprocess.run(["which", tool], capture_output=True).returncode != 0:
        sys.exit(f"missing tool: {tool}")


def run(cmd: list[str]) -> str:
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"failed: {' '.join(cmd)}\n{p.stderr.strip()}")
    return p.stdout


def normalise(text: str) -> str:
    """Collapse whitespace: OCR line breaks are not the thing under test."""
    return re.sub(r"\s+", "", text)


def accuracy(truth: str, got: str) -> float:
    """Order-sensitive similarity. Collapses when the two sides read columns in
    a different order, so it is a floor, not the recognition rate."""
    if not truth:
        return float("nan")
    return difflib.SequenceMatcher(None, truth, got).ratio()


def char_f1(truth: str, got: str) -> float:
    """Order-free character F1.

    This is the one that answers "did it read the ink": a two-column paper comes
    back with every word correct and still scores badly on the sequence metric,
    because pdftotext and the detector weave the columns together differently.
    Counting characters instead isolates recognition from layout.
    """
    if not truth:
        return float("nan")
    t, g = collections.Counter(truth), collections.Counter(got)
    overlap = sum((t & g).values())
    if not overlap:
        return 0.0
    precision, recall = overlap / max(len(got), 1), overlap / len(truth)
    return 2 * precision * recall / (precision + recall)


def score_pdf(pdf: pathlib.Path, dpi: int, lang: str, max_pages: int, engine: str, cache: pathlib.Path, order: str, truth: str) -> list[tuple[int, int, float, float]]:
    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = pathlib.Path(tmp)
        # Ground truth: the embedded text layer, one file per page.
        truth_cmd = ["pdftotext"]
        if truth == "layout":
            # Preserves the visual page: for a two-column paper that means weaving
            # the columns together across each visual line, which is NOT reading order.
            truth_cmd.append("-layout")
        # Page breaks are kept either way: the scorer splits truth per page on \f.
        run(truth_cmd + [str(pdf), str(tmpdir / "truth.txt")])
        cmd = ["pdftoppm", "-r", str(dpi), "-png", str(pdf), str(tmpdir / "page")]
        if max_pages:
            cmd[1:1] = ["-l", str(max_pages)]
        run(cmd)
        images = sorted(tmpdir.glob("page-*.png"))
        if not images:
            return []

        truths = (tmpdir / "truth.txt").read_text(errors="ignore").split("\f")

        # Cache raw blocks, not joined text: reordering happens after OCR, so one
        # cache serves every ordering and switching --order costs nothing.
        cached = cache / f"{pdf.stem}-{dpi}-{engine}.json"
        if cached.exists():
            pages = [json.loads(c) for c in cached.read_text(errors="ignore").split("\0") if c.strip()]
        else:
            pages = ocr_all(images, lang, engine)
            cache.mkdir(parents=True, exist_ok=True)
            cached.write_text("\0".join(json.dumps(p) for p in pages))
        texts = ["\n".join(b["text"] for b in (reading_order.order_blocks(p) if order == "column" else p))
                 for p in pages]
        rows = []
        for i in range(len(images)):
            got = texts[i] if i < len(texts) else ""
            truth = truths[i] if i < len(truths) else ""
            nt, ng = normalise(truth), normalise(got)
            rows.append((i + 1, len(nt), accuracy(nt, ng), char_f1(nt, ng)))
        return rows


def ocr_all(images: list[pathlib.Path], lang: str, engine: str) -> list[str]:
    """Recognise every page, in order, with one model load for the whole batch."""
    if engine == "tesseract":
        out = []
        for image in images:
            base = image.with_suffix("")
            run(["tesseract", str(image), str(base), "-l", lang])
            text = pathlib.Path(f"{base}.txt").read_text(errors="ignore")
            out.append([{"box": [[0, 0], [0, 0], [0, 0], [0, 0]], "text": text, "score": 0.0}])
        return out

    here = pathlib.Path(__file__).resolve().parent
    venv = here / ".venv-ocr/bin/python"
    if not venv.exists():
        sys.exit(f"ppocr needs the measurement venv: {venv}")
    # NUL-separated: OCR text is full of newlines and tabs, so it cannot be the delimiter.
    chunks = run([str(venv), str(here / "ocr-ppocr.py")] + [str(i) for i in images]).split("\0")
    return [json.loads(c) for c in chunks if c.strip()]


def collect(paths: list[str]) -> list[pathlib.Path]:
    out = []
    for raw in paths:
        p = pathlib.Path(raw).expanduser()
        if p.is_dir():
            out.extend(sorted(p.glob("*.pdf")))
        elif p.suffix.lower() == ".pdf":
            out.append(p)
        else:
            print(f"  skip (not a pdf): {p}", file=sys.stderr)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--dpi", type=int, default=300)
    ap.add_argument("--lang", default="eng")
    ap.add_argument("--max-pages", type=int, default=0, help="0 = all pages")
    ap.add_argument("--cache", default=".ocr-cache", help="reuse OCR output between runs")
    ap.add_argument("--truth", choices=["flow", "layout"], default="flow",
                    help="flow = pdftotext reading-order mode (default); layout = visual page")
    ap.add_argument("--order", choices=["raw", "column"], default="column",
                    help="column = undo the detector's column weaving (default)")
    ap.add_argument("--engine", choices=["tesseract", "ppocr"], default="tesseract",
                    help="ppocr is the intended engine; tesseract only smoke-tests the pipeline")
    args = ap.parse_args()

    for tool in (TOOLS if args.engine == "tesseract" else ("pdftotext", "pdftoppm")):
        need(tool)

    pdfs = collect(args.paths)
    if not pdfs:
        sys.exit("no PDFs found")

    print(f"OCR baseline  engine={args.engine}  dpi={args.dpi}  lang={args.lang}  order={args.order}  truth={args.truth}  documents={len(pdfs)}")
    print()
    print(f"  {'document':<40} {'page':>4} {'chars':>7} {'seq':>7} {'charF1':>8}")

    all_scores: list[float] = []
    all_seq: list[float] = []
    for pdf in pdfs:
        rows = score_pdf(pdf, args.dpi, args.lang, args.max_pages, args.engine,
                         pathlib.Path(args.cache).expanduser(), args.order, args.truth)
        if not rows:
            print(f"  {pdf.name[:43]:<44} {'-':>4} {'-':>7} {'no pages':>9}")
            continue
        for page, chars, acc, f1 in rows:
            flag = "" if f1 >= 0.95 else ("  <- weak" if f1 >= 0.85 else "  <- poor")
            print(f"  {pdf.name[:39]:<40} {page:>4} {chars:>7} {acc:>6.1%} {f1:>7.1%}{flag}")
            if chars >= 20:  # ignore near-blank pages when averaging
                all_scores.append(f1)
                all_seq.append(acc)

    print()
    if all_scores:
        mean = sum(all_scores) / len(all_scores)
        mean_seq = sum(all_seq) / len(all_seq) if all_seq else float("nan")
        print(f"  pages scored: {len(all_scores)}   mean charF1: {mean:.1%}   mean seq: {mean_seq:.1%}")
        print("  (charF1 is order-free and should not move; seq is what reading order fixes)")
        print(f"  verdict: {'PASS' if mean >= 0.95 else 'FAIL'} (threshold 95%)")
    else:
        print("  no page had enough text to score")


if __name__ == "__main__":
    main()
