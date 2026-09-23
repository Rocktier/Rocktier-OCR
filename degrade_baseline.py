#!/usr/bin/env python3
"""How much of the gate survives a real scan?

The published gate - 97.5% character F1 - was measured on clean digital pages
rendered to images. Real scans carry skew, noise, JPEG mush and low resolution,
and the README already says they will score lower. This measures how much lower,
by degrading the same pages and re-recognising them, so the gate has a floor as
well as a ceiling.

    .venv-ocr/bin/python degrade_baseline.py [--docs 8]

Every number here is relative: what matters is the drop from clean, not the
absolute value, because the corpus is still digital pages rather than paper.
"""

import argparse
import io
import pathlib
import json
import statistics
import subprocess
import sys

import numpy as np
from PIL import Image

import baseline

ROOT = pathlib.Path(__file__).resolve().parent
DPI = 200

# Ordered roughly by how often a real scan carries each one.
KINDS = ("clean", "skew1", "skew2", "jpeg40", "dpi150", "noise")
LABEL = {
    "clean": "干净（基准）",
    "skew1": "倾斜 1.0°",
    "skew2": "倾斜 2.0°",
    "jpeg40": "JPEG 质量 40",
    "dpi150": "降到 150 dpi",
    "noise": "高斯噪声 σ=8",
}


def rasterise(pdf: pathlib.Path, prefix: pathlib.Path) -> pathlib.Path:
    subprocess.run(
        ["pdftoppm", "-f", "1", "-l", "1", "-r", str(DPI), "-png", str(pdf), str(prefix)],
        check=True, capture_output=True,
    )
    out = sorted(prefix.parent.glob(f"{prefix.name}*.png"))
    if not out:
        raise SystemExit(f"could not rasterise {pdf}")
    return out[0]


def truth_of(pdf: pathlib.Path) -> str:
    t = subprocess.run(
        ["pdftotext", "-f", "1", "-l", "1", str(pdf), "-"],
        capture_output=True, text=True,
    ).stdout
    return baseline.normalise(t)


def degrade(img: Image.Image, kind: str) -> Image.Image:
    if kind == "clean":
        return img
    if kind == "skew1":
        return img.rotate(1.0, resample=Image.BICUBIC, expand=False, fillcolor=255)
    if kind == "skew2":
        return img.rotate(2.0, resample=Image.BICUBIC, expand=False, fillcolor=255)
    if kind == "jpeg40":
        buf = io.BytesIO()
        img.convert("RGB").save(buf, "JPEG", quality=40)
        return Image.open(buf).convert("RGB")
    if kind == "dpi150":
        s = 150.0 / DPI
        return img.resize((max(1, int(img.width * s)), max(1, int(img.height * s))),
                          Image.Resampling.LANCZOS)
    if kind == "noise":
        a = np.asarray(img.convert("RGB")).astype(np.int16)
        a = a + np.random.normal(0, 8, a.shape).astype(np.int16)
        return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
    raise SystemExit(f"unknown degradation {kind}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--docs", type=int, default=8, help="how many documents to use")
    ap.add_argument("--corpus", default="corpus")
    a = ap.parse_args()

    docs = sorted((ROOT / a.corpus).glob("*.pdf"))[: a.docs]
    if not docs:
        raise SystemExit(f"no pdfs in {ROOT / a.corpus}")
    print(f"  语料 {len(docs)} 份，每份第 1 页，{DPI} dpi")
    print(f"  退化类型 {len(KINDS)} 种 × {len(docs)} 份 = {len(KINDS) * len(docs)} 次识别\n")

    work = pathlib.Path("/tmp/ocr-degrade")
    work.mkdir(exist_ok=True)
    src = work / "src"
    src.mkdir(exist_ok=True)

    truths = {}
    pages = {}
    for pdf in docs:
        pages[pdf.stem] = Image.open(rasterise(pdf, src / pdf.stem))
        truths[pdf.stem] = truth_of(pdf)

    results = {}
    for kind in KINDS:
        stems = list(pages.keys())
        imgs = []
        for stem, img in pages.items():
            p = work / f"{stem}-{kind}.png"
            degrade(img, kind).save(p)
            imgs.append(p)
        # One batch per kind, so the model is loaded once rather than per image.
        recognised = baseline.ocr_all(imgs, "eng", "ppocr")
        scores = []
        for stem, blocks in zip(stems, recognised):
            got = baseline.normalise("\n".join(b["text"] for b in (blocks or [])))
            scores.append(baseline.char_f1(truths[stem], got))
        results[kind] = scores
        print(f"  {LABEL[kind]:<16} 均值 {statistics.mean(scores) * 100:6.2f}%   "
              f"(min {min(scores) * 100:.2f} / max {max(scores) * 100:.2f})")

    base = statistics.mean(results["clean"])
    print()
    print(f"  基准（干净） {base * 100:.2f}%   —— 与 README 公布的 97.5% 对照")
    print()
    print(f"  {'退化':<16} {'字符 F1':>8} {'相对基准':>10}")
    for kind in KINDS:
        m = statistics.mean(results[kind])
        d = m - base
        print(f"  {LABEL[kind]:<16} {m * 100:7.2f}% {d * 100:+8.2f}pt")
    worst = min(KINDS, key=lambda k: statistics.mean(results[k]))
    print()
    print(f"  最致命的一项: {LABEL[worst]}（{statistics.mean(results[worst]) * 100:.2f}%）")
    print("  门是 95% —— 低于它，'比免费的差还收费'就不成立")
    out = pathlib.Path("/tmp/ocr-degrade/results.json")
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps({k: v for k, v in results.items()}, indent=1))
    print(f"  原始数值已存 {out}")


if __name__ == "__main__":
    main()
