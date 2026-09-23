#!/usr/bin/env python3
"""Verify a generated searchable PDF against the acceptance criteria.

Automated on purpose: criteria 3 (word-box alignment) and 5 (pixel-identical
output) cannot be checked by eye, and eyeballing "you can select the text" would
have let 603 words silently become 319.

    python3 check_text_layer.py <stem> [--dpi 200]

Uses the OCR cache, so it does not re-recognise anything.
"""

import argparse
import difflib
import json
import pathlib
import re
import shutil
import subprocess

from collections import Counter

ROOT = pathlib.Path(__file__).resolve().parent

import reading_order  # noqa: E402  (same directory)
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
    ap.add_argument("--order", choices=["raw", "column"], default="column")
    a = ap.parse_args()

    cached = ROOT / a.cache / f"{a.stem}-{a.dpi}-ppocr.json"
    if not cached.exists():
        raise SystemExit(f"no OCR cache at {cached} - run baseline.py first")
    blocks = json.loads(cached.read_text(errors="ignore").split("\0")[0])
    if a.order == "column":
        # The write side does not reorder: it draws what it is handed, in order.
        # Reading order is applied here, so both halves stay independently testable.
        blocks = reading_order.order_blocks(blocks)

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

    def norm(t):
        return re.sub(r"\s+", "", t)

    ocr_text = " ".join(b["text"] for b in blocks)
    print(f"  {a.stem}  page {width}x{height}px = {page_pt:.0f}pt  blocks {len(blocks)}")

    # 2 - round trip. Measured order-free on purpose: pdftotext rebuilds order from
    # glyph geometry, so comparing our reading-order text against it would charge the
    # write side for a layout difference that belongs to the detector. The
    # order-sensitive number is reported next to it for reference, not as a verdict.
    def words_of(t):
        return [w for w in re.split(r"\s+", t) if w]

    ocr_w, got_w = Counter(words_of(ocr_text)), Counter(words_of(text))
    recovered = sum(min(ocr_w[w], got_w[w]) for w in ocr_w) / max(1, sum(ocr_w.values()))
    sequence = difflib.SequenceMatcher(None, norm(ocr_text), norm(text)).ratio()
    missing = {w: (n - got_w.get(w, 0)) for w, n in ocr_w.items() if got_w.get(w, 0) < n}
    missing_n = sum(missing.values())
    ascii_ocr = {w: n for w, n in ocr_w.items() if all(ord(c) < 128 for c in w)}
    ascii_recovered = (sum(min(ascii_ocr[w], got_w.get(w, 0)) for w in ascii_ocr)
                       / max(1, sum(ascii_ocr.values())))
    print(f"  2. text     OCR {ocr_words} words -> extracted {got_words}   "
          f"round-trip recovery {recovered:.1%} "
          f"({'PASS' if recovered >= 0.99 else 'FAIL'}, milestone says >=99%)")
    # Where the missing words went. Two causes behave completely differently: a word
    # swallowed by its neighbour is a spacing problem, a word with a non-Latin-1
    # character in it is a font problem, and lumping them together hides both.
    merged = sum(n for w, n in missing.items() if any(w in e and e != w for e in got_w))
    nonascii = sum(n for w, n in missing.items() if any(ord(c) > 127 for c in w))
    print(f"        of the {missing_n} lost: {merged} merged into a neighbour, "
          f"{nonascii} carry a non-Latin-1 character, "
          f"{missing_n - merged - nonascii} other")
    print(f"        ASCII-only words: {ascii_recovered:.1%} recovered")

    # 3 - split in two, which is the decision of 2026-09-19. The widest box comes
    # from tokens the detector emitted with no space in them ("Theseresultsdemo");
    # the writer draws them faithfully, so charging that to the write side is what
    # made this criterion fail forever.
    if boxes:
        widths = sorted(b for b, _ in boxes)
        median = widths[len(widths) // 2]
        widest = max(widths)
        sticky = sum(1 for b in widths if b / page_pt > 0.15)
        print(f"  3a. write-side boxes  {len(boxes)} boxes, median {median:.1f}pt "
              f"({'PASS' if median > 4.0 else 'FAIL'}, need >2pt and >4pt)"
              f"{'   <- degenerate boxes' if median <= 2.0 else ''}")
        print(f"  3b. recognition stickiness  {sticky}/{len(boxes)} tokens wider than 15% of page "
              f"(widest {widest / page_pt:.1%})   <- the recognition side owns this, "
              f"not this milestone")
    else:
        print("  3a. write-side boxes FAIL - nothing extracted")

    print(f"  4. single layer: {'PASS' if got_words < ocr_words * 1.5 else 'FAIL'} (a stacked layer would roughly double)")
    # 5 - measured rather than claimed. The page image goes in as a DCTDecode stream
    # with no re-encoding, so pulling it back out and comparing the bytes shows
    # whether anything visible was added; render mode 3 only asserts it.
    if not shutil.which("pdfimages"):
        visual = "not checked - pdfimages missing"
    else:
        for stale in work.glob("img*"):
            stale.unlink()
        subprocess.run(["pdfimages", "-j", str(out), str(work / "img")], check=True)
        imgs = sorted(work.glob("img*.jpg"))
        visual = (f"PASS - embedded image byte-identical to the page that went in "
                  f"({jpg.stat().st_size} B)"
                  if imgs and imgs[0].read_bytes() == jpg.read_bytes()
                  else f"FAIL - the embedded image changed ({len(imgs)} extracted, "
                       f"{imgs[0].stat().st_size if imgs else 0} B vs {jpg.stat().st_size} B)")
    print(f"  5. visual  {visual}")

    # 6 - geometric, replacing the order criterion that was judged invalid. Compare
    # the boxes the reader reports against the ones the writer says it placed. Word
    # sequences are aligned by text first: pdftotext rebuilds order from geometry,
    # so it will not come back in the order the words were written.
    sidecar = pathlib.Path(str(out) + ".boxes.json")
    if not sidecar.exists():
        print("  6. geometry FAIL - the writer produced no sidecar")
    else:
        side = json.loads(sidecar.read_text())
        bwords = re.findall(
            r'<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)"[^>]*>([^<]*)</word>',
            bbox)
        bwords = [(t, float(x0), float(y0), float(x1), float(y1)) for x0, y0, x1, y1, t in bwords]
        sm = difflib.SequenceMatcher(None, [s["text"] for s in side], [b[0] for b in bwords],
                                     autojunk=False)
        devs = []
        for i, j, n in sm.get_matching_blocks():
            for k in range(n):
                devs.append(abs(side[i + k]["x0"] - bwords[j + k][1]) / page_pt)
        if devs:
            devs.sort()
            med = devs[len(devs) // 2]
            print(f"  6. geometry  {len(devs)}/{len(side)} words matched, "
                  f"median |dx| {med:.2%} of page width "
                  f"({'PASS' if med < 0.02 else 'FAIL'}, need <2%)")

            # 7 - will a viewer keep the words apart? This is the gap as a fraction of
            # the font size, because that ratio is what decides it. poppler breaks
            # words at roughly a tenth of the font size, but the readers people
            # actually open PDFs in want more: at 0.15 em a title came back as
            # REFINEMENTISINHERENTLYEDITABLE in Edge while pdftotext saw every space.
            # Grading this with poppler alone is how that went unnoticed.
            tightest = None
            for s1, s2 in zip(side, side[1:]):
                if abs(s1["y0"] - s2["y0"]) < 0.5 and s2["x0"] > s1["x0"]:
                    h = s2["y1"] - s2["y0"]
                    if h > 0.5:
                        g = (s2["x0"] - s1["x1"]) / h
                        tightest = g if tightest is None else min(tightest, g)
            if tightest is not None:
                print(f"  7. gap      tightest {tightest:.3f} em "
                      f"({'PASS' if tightest >= 0.2 else 'FAIL'}, viewers want >=0.2 em; "
                      f"poppler alone needs only ~0.1)")
        else:
            print("  6. geometry FAIL - no word could be matched to a written box")
    print("  1. selectable: not automatable - open it in Preview once")


if __name__ == "__main__":
    main()
