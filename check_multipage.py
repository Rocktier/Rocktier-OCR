#!/usr/bin/env python3
"""The acceptance criteria, for a whole document.

Seven of them were written when the writer produced one page. The writer has
moved past that: it now opens the original PDF and layers text onto every page,
optionally replacing the text that was already there. This is the same
yardstick, applied per page, plus the two questions only a whole document can
answer - did every page survive, and can the words be found again.

    .venv-ocr/bin/python check_multipage.py <pdf> [--dpi 200] [--max-pages 5] [--no-replace]

Pages that were not recognised keep their text, so on those pages the stacking
check is expected to show the original words. --max-pages keeps the runtime
sane: recognition, not writing, is what costs time here.
"""

import argparse
import difflib
import json
import pathlib
import re
import statistics
import os
import subprocess
import sys

from collections import Counter
from PIL import Image, ImageChops

import baseline

# 验收要做几何比对（判据 6），写出器默认不留 sidecar，这里打开。
os.environ["ROCKTIER_OCR_BOXES"] = "1"

ROOT = pathlib.Path(__file__).resolve().parent
WRITER = ROOT / "writer/target/release/write-searchable"
TEXT_OPS = ("Tj", "TJ", "'", '"')


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, cmd))}\n{p.stderr.strip()[:300]}")
    return p.stdout


def words_of(t):
    return [w for w in re.split(r"\s+", t) if w]


def norm(t):
    return re.sub(r"\s+", "", t)


def page_text(pdf, i):
    return run(["pdftotext", "-f", str(i), "-l", str(i), str(pdf), "-"])


def page_bbox_words(pdf, i):
    bbox = run(["pdftotext", "-f", str(i), "-l", str(i), "-bbox", str(pdf), "-"])
    return re.findall(
        r'<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)"[^>]*>([^<]*)</word>',
        bbox)


def raster(pdf, i, dpi, prefix):
    run(["pdftoppm", "-f", str(i), "-l", str(i), "-r", str(dpi), "-png", str(pdf), str(prefix)])
    hits = sorted(prefix.parent.glob(f"{prefix.name}*.png"))
    return hits[0]


def mean_abs_diff(a: pathlib.Path, b: pathlib.Path) -> float:
    """Mean absolute pixel difference between two images. 0 = identical."""
    ia, ib = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
    if ia.size != ib.size:
        return -1.0
    d = ImageChops.difference(ia, ib)
    h = d.histogram()  # RGB: 768 bins, 256 per channel
    n = ia.size[0] * ia.size[1]
    total = 0.0
    for ch in range(3):
        band = h[ch * 256 : (ch + 1) * 256]
        total += sum(v * c for v, c in enumerate(band))
    return total / (n * 3)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf")
    ap.add_argument("--dpi", type=int, default=200)
    ap.add_argument("--max-pages", type=int, default=5)
    ap.add_argument("--no-replace", action="store_true")
    a = ap.parse_args()

    pdf = pathlib.Path(a.pdf)
    work = pathlib.Path("/tmp/ocr-mp")
    work.mkdir(exist_ok=True)
    for stale in work.glob("*"):
        stale.unlink()

    # Rasterise and recognise every page first - the writer needs one entry per page.
    run(["pdftoppm", "-r", str(a.dpi), "-png", str(pdf), str(work / "in")])
    images = sorted(work.glob("in-*.png"))
    if a.max_pages:
        images = images[: a.max_pages]
    n = len(images)
    print(f"  {pdf.name}   处理 {n} 页（{a.dpi} dpi，{'替换' if not a.no_replace else '追加'}模式）\n")

    pages = baseline.ocr_all(images, "eng", "ppocr")
    pages_json = work / "pages.json"
    pages_json.write_text(json.dumps(pages))

    out = work / "layered.pdf"
    cmd = [str(WRITER), "--pdf", str(pdf), str(pages_json), str(out), str(a.dpi)]
    if not a.no_replace:
        cmd.append("replace")
    subprocess.run(cmd, check=True, capture_output=True, text=True)

    side = json.loads(pathlib.Path(str(out) + ".boxes.json").read_text())
    by_page = {}
    for w in side:
        by_page.setdefault(w.get("page", 0), []).append(w)

    per_page = {i: baseline.normalise(page_text(pdf, i)) for i in range(1, n + 1)}
    all_ocr_words, all_got_words = Counter(), Counter()
    rows = []

    print(f"  {'页':>3} {'复现率':>8} {'合并':>5} {'3a中位':>8} {'几何|dx|':>9} {'最紧间距':>9} {'视觉':>8}")
    for i in range(1, n + 1):
        blocks = pages[i - 1] or []
        ocr_text = " ".join(b["text"] for b in blocks)
        got = page_text(out, i)
        ocw, gcw = Counter(words_of(ocr_text)), Counter(words_of(got))
        all_ocr_words += ocw
        all_got_words += gcw
        recovered = sum(min(ocw[w], gcw[w]) for w in ocw) / max(1, sum(ocw.values()))
        missing = {w: c - gcw.get(w, 0) for w, c in ocw.items() if gcw.get(w, 0) < c}
        merged = sum(c for w, c in missing.items() if any(w in e and e != w for e in gcw))

        sw = by_page.get(i, [])
        widths = sorted((w["x1"] - w["x0"]) for w in sw)
        med_box = widths[len(widths) // 2] if widths else 0.0

        img_w = Image.open(images[i - 1]).width
        page_pt = img_w * 72.0 / a.dpi
        bw = page_bbox_words(out, i)
        bwords = [(t, float(x0), float(y0), float(x1), float(y1)) for x0, y0, x1, y1, t in bw]
        sm = difflib.SequenceMatcher(None, [s["text"] for s in sw],
                                     [b[0] for b in bwords], autojunk=False)
        devs = []
        for x, y, k in sm.get_matching_blocks():
            for j in range(k):
                devs.append(abs(sw[x + j]["x0"] - bwords[y + j][1]) / page_pt)
        devs.sort()
        med_dx = devs[len(devs) // 2] if devs else float("nan")

        gaps = []
        for s1 in sw:
            near = None
            for s2 in sw:
                if s2 is s1 or s2["x0"] <= s1["x1"]:
                    continue
                if not (s1["y0"] < s2["y1"] - 0.01 and s1["y1"] > s2["y0"] + 0.01):
                    continue
                if near is None or s2["x0"] < near["x0"]:
                    near = s2
            if near is not None:
                h = near["y1"] - near["y0"]
                if h > 0.5:
                    gaps.append((near["x0"] - s1["x1"]) / h)
        tightest = min(gaps) if gaps else float("nan")

        src = raster(pdf, i, a.dpi, work / f"src{i}")
        dst = raster(out, i, a.dpi, work / f"out{i}")
        vis = mean_abs_diff(src, dst)

        rows.append((i, recovered, merged, med_box, med_dx, tightest, vis))
        print(f"  {i:>3} {recovered:7.1%} {merged:>5} {med_box:7.1f}pt "
              f"{med_dx:8.3%} {tightest:8.3f}em {vis:7.2f}")

    tot_ocr = sum(all_ocr_words.values())
    tot_got = sum(all_got_words.values())
    recovered = sum(min(all_ocr_words[w], all_got_words[w]) for w in all_ocr_words) / max(1, tot_ocr)
    sample = sorted(set(w for w in all_ocr_words if len(w) > 1))[:300]
    whole_text = run(["pdftotext", str(out), "-"])
    found = sum(1 for w in sample if w in whole_text)

    print()
    print(f"  页数            输入 {info(pdf)}  →  输出 {info(out)}")
    print(f"  2  往返复现率    {recovered:.1%}   （里程碑 ≥99%）")
    print(f"     合并词合计    {sum(r[2] for r in rows)}")
    print(f"  3a  框健康度     均值 {statistics.mean(r[3] for r in rows):.1f}pt")
    print(f"  4  不叠加        抽回 {tot_got} / 识别 {tot_ocr} "
          f"({'PASS' if tot_got < tot_ocr * 1.5 else 'FAIL'}，叠加会接近两倍)")
    print(f"  5  视觉零改变    均值差 {statistics.mean(r[6] for r in rows):.3f} "
          f"({'PASS' if statistics.mean(r[6] for r in rows) < 1.0 else 'FAIL'}，0 = 逐像素相同)")
    print(f"  6  几何偏差      均值 {statistics.mean(r[4] for r in rows):.3%}")
    print(f"  7  最紧间距      均值 {statistics.mean(r[5] for r in rows):.3f} em  （判据 7 已标注不可靠，仅供参考）")
    print(f"  8  搜索召回      {found}/{len(sample)} 个抽样词能被找到 "
          f"({found / max(1, len(sample)):.1%}；子串匹配，合并词大多仍可命中)")
    print()
    print(f"  判据 1（可选中）不可自动化 —— 用阅读器打开 {out} 抽几行核对")


def info(pdf):
    o = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True).stdout
    for line in o.splitlines():
        if line.startswith("Pages:"):
            return line.split(":", 1)[1].strip()
    return "?"


if __name__ == "__main__":
    main()
