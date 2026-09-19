#!/usr/bin/env python3
"""Per-page comparison of the detector's raw order against column reordering.

Exists because a mean hides the thing that actually matters: a change can lift
five pages and wreck twelve, and the average will say "roughly unchanged" either
way. That is exactly how a regression got past me once. Run this before and after
every change to reading_order.py.

Reads the OCR cache, so it is instant - no images, no model, no waiting.

    python3 compare_orders.py corpus [--dpi 200] [--truth flow]
"""

import argparse
import difflib
import json
import pathlib
import re
import subprocess

import reading_order


def norm(text):
    return re.sub(r"\s+", "", text)


def seq(truth, got):
    if not truth:
        return float("nan")
    return difflib.SequenceMatcher(None, truth, got).ratio()


def truth_pages(pdf, layout):
    cmd = ["pdftotext"] + (["-layout"] if layout else [])
    out = subprocess.run(cmd + [str(pdf), "-"], capture_output=True, text=True)
    return out.stdout.split("\f")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--dpi", type=int, default=200)
    ap.add_argument("--engine", default="ppocr")
    ap.add_argument("--cache", default=".ocr-cache")
    ap.add_argument("--truth", choices=["flow", "layout"], default="flow")
    a = ap.parse_args()

    cache = pathlib.Path(a.cache)
    files = sorted(p for d in a.paths for p in pathlib.Path(d).expanduser().glob("*.pdf"))
    if not files:
        raise SystemExit("no PDFs found")

    print(f"  {'document':<34} {'page':>4} {'chars':>6} {'raw':>7} {'col':>7} {'delta':>7}")
    better = worse = n = 0
    total = 0.0
    for pdf in files:
        cached = cache / f"{pdf.stem}-{a.dpi}-{a.engine}.json"
        if not cached.exists():
            print(f"  {pdf.name[:33]:<34} {'-':>4} {'':>6} {'':>7} {'':>7} {'no cache':>7}")
            continue
        pages = [json.loads(c) for c in cached.read_text(errors="ignore").split("\0") if c.strip()]
        truths = truth_pages(pdf, a.truth == "layout")
        for i, blocks in enumerate(pages):
            truth = norm(truths[i]) if i < len(truths) else ""
            if len(truth) < 20:  # near-blank pages are noise
                continue
            raw = seq(truth, norm("\n".join(b["text"] for b in blocks)))
            col = seq(truth, norm("\n".join(b["text"] for b in reading_order.order_blocks(blocks))))
            delta = col - raw
            flag = "  better" if delta > 0.005 else ("  WORSE" if delta < -0.005 else "")
            print(f"  {pdf.name[:33]:<34} {i + 1:>4} {len(truth):>6} {raw:>6.1%} {col:>6.1%} {delta:>+6.1%}{flag}")
            total += delta
            n += 1
            better += delta > 0.005
            worse += delta < -0.005

    print()
    print(f"  pages: {n}   better: {better}   worse: {worse}   mean delta: {total / max(n, 1):+.1%}")


if __name__ == "__main__":
    main()
