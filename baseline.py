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
import difflib
import pathlib
import re
import subprocess
import sys
import tempfile

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
    """1 - (edit distance / truth length), i.e. character accuracy."""
    if not truth:
        return float("nan")
    return difflib.SequenceMatcher(None, truth, got).ratio()


def score_pdf(pdf: pathlib.Path, dpi: int, lang: str, max_pages: int) -> list[tuple[int, int, float]]:
    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = pathlib.Path(tmp)
        # Ground truth: the embedded text layer, one file per page.
        run(["pdftotext", "-layout", str(pdf), str(tmpdir / "truth.txt")])
        cmd = ["pdftoppm", "-r", str(dpi), "-png", str(pdf), str(tmpdir / "page")]
        if max_pages:
            cmd[1:1] = ["-l", str(max_pages)]
        run(cmd)
        images = sorted(tmpdir.glob("page-*.png"))
        if not images:
            return []

        truths = (tmpdir / "truth.txt").read_text(errors="ignore").split("\f")

        rows = []
        for i, image in enumerate(images):
            base = tmpdir / f"out-{i}"
            run(["tesseract", str(image), str(base), "-l", lang])
            got = (pathlib.Path(f"{base}.txt")).read_text(errors="ignore")
            truth = truths[i] if i < len(truths) else ""
            rows.append((i + 1, len(normalise(truth)), accuracy(normalise(truth), normalise(got))))
        return rows


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
    args = ap.parse_args()

    for tool in TOOLS:
        need(tool)

    pdfs = collect(args.paths)
    if not pdfs:
        sys.exit("no PDFs found")

    print(f"OCR baseline  dpi={args.dpi}  lang={args.lang}  documents={len(pdfs)}")
    print()
    print(f"  {'document':<44} {'page':>4} {'chars':>7} {'accuracy':>9}")

    all_scores: list[float] = []
    for pdf in pdfs:
        rows = score_pdf(pdf, args.dpi, args.lang, args.max_pages)
        if not rows:
            print(f"  {pdf.name[:43]:<44} {'-':>4} {'-':>7} {'no pages':>9}")
            continue
        for page, chars, acc in rows:
            flag = "" if acc >= 0.95 else ("  <- weak" if acc >= 0.7 else "  <- poor")
            print(f"  {pdf.name[:43]:<44} {page:>4} {chars:>7} {acc:>8.1%}{flag}")
            if chars >= 20:  # ignore near-blank pages when averaging
                all_scores.append(acc)

    print()
    if all_scores:
        mean = sum(all_scores) / len(all_scores)
        print(f"  pages scored: {len(all_scores)}   mean accuracy: {mean:.1%}")
        print(f"  verdict: {'PASS' if mean >= 0.95 else 'FAIL'} (threshold 95%)")
    else:
        print("  no page had enough text to score")


if __name__ == "__main__":
    main()
