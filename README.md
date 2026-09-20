# Rocktier OCR

Makes scans and screenshots searchable. Offline, private, small, one-time purchase.

**Status: gate passed; the write side is being built.** There is still no user
interface, and deliberately so. The baseline that decides whether this project is worth
building at all came back at 97.5% character F1 on real English papers (see *Gate*
below), which moved the work onto the piece everything else depends on: **producing a
searchable PDF at all** — placing a hidden text layer, and measuring text widths from
the reader rather than estimating them.

## Scope, and what is out of scope

In: turn a scan or screenshot into selectable text and a searchable PDF.

Out: data extraction. Pulling structured fields out of documents is the territory of
cloud VLM products (DeepSeek-OCR, MinerU, olmOCR) and would drag this app into the
network, into per-page costs, and out of the family's "files never leave the machine"
promise.

## Licence red lines

This ships closed-source and paid, so every dependency has to permit that:

| Allowed | Licence |
|---|---|
| Tesseract, PaddleOCR, RapidOCR, Surya | Apache-2.0 |
| DeepSeek-OCR | MIT |

**MinerU is AGPL-3.0 and must never be used or linked.** If a component's licence is not
listed above, check it before it goes anywhere near this repo.

## Engine

Modern OCR engines are almost all Python, and shipping a Python runtime would cost the
"tiny native app" signature the family is built on. The intended path is
**Rust + ONNX Runtime + PP-OCR**. Tesseract is on the machine and is fine for
smoke-testing the pipeline, but its numbers are *not* the product's numbers.

## Gate: the accuracy baseline

Do not write UI before this passes. Run a real English document set through the intended
engine and measure:

```sh
python3 baseline.py <pdf-or-dir> --dpi 300 --lang eng
```

Ground truth is each PDF's own text layer, discarded from the image before recognition,
so the comparison costs nothing and involves no labelling. Pages with almost no text are
skipped so near-blank pages cannot flatter the mean.

**Threshold: 95% mean character F1** on clean scans. Below that, stop and reconsider —
the market has ABBYY at $199 and a free 47k-star alternative; "worse and paid" is not a
position.

## Baseline result (2026-09-19): PASS, 97.5%

PP-OCR via `rapidocr-onnxruntime` (Apache-2.0, models bundled), 20 real English papers
from arXiv cs.CV, page 1 of each, rasterised at 200 dpi to `corpus/`.

| metric | mean | what it measures |
|---|---|---|
| char F1 (order-free) | **97.5%** | whether the ink was read |
| sequence ratio (order-sensitive) | 82.8% | read *and laid out* as pdftotext sees it |

The two disagree wildly on individual documents — one page scores 64.0% by sequence and
97.8% by character. The gap is **reading order**, not recognition: two-column papers get
woven together differently by the detector and by `pdftotext -layout`, so a page with
every word correct still scores badly on the sequence metric. Recognition passes; layout
does not, yet.

**Two caveats that keep this from being the final word:**

1. These are clean digital pages rendered to images, not photographs of paper. Real scans
   carry skew, noise, JPEG mush and coffee stains, and will score lower. A real-scan set
   is still owed before the verdict is defensible.
2. Only page 1 of each document was measured, and F1 ignores reading order. Putting the
   text back in human reading order is a real work item — most likely **column
   detection** (PP-Structure, Apache-2.0) — and belongs in the plan, not in the "done"
   column.

Raw per-page numbers: `python3 baseline.py corpus --engine ppocr --dpi 200 --max-pages 1`
(OCR output is cached in `.ocr-cache/`, so re-scoring is instant).

## Chinese baseline (2026-09-19): 93.7%, just under the gate

Same engine and settings on 6 real Chinese business PDFs (product selection lists and
catalogues, pulled from the operator's own Downloads — every one of them a document type
this product would actually see).

| document | char F1 |
|---|---|
| 浪鲸选品 | 98.3% |
| 万达公馆选品清单 | 98.2% |
| 浪鲸卫浴选品清单 | 96.2% |
| 九牧育才名苑 | 94.9% |
| 禾止美学 | 94.2% |
| 武汉冈森水槽图册 | 80.3% |
| **mean** | **93.7%** |

Unlike the English set, sequence ratio and char F1 agree here (94.9 vs 94.9, 98.3 vs 98.3),
so reading order is *not* distorting this number — these are single-column tables. That
makes 93.7% a cleaner recognition figure than the English 97.5%.

**It is still probably a floor, and that matters before anyone calls it a fail.**
`pdftotext` extracting CJK depends on each font carrying a usable ToUnicode table; where
it does not, the "ground truth" is itself wrong and the engine is charged for it. The
dense, small-font catalogue that scored 80.3% is exactly the shape of document where that
happens. Isolating truth errors from recognition errors is the next measurement, not a
conclusion to draw now.

## The question this raises — ANSWERED (2026-09-19)

**Decision: English-first stands. The operator settled it directly: "我们还是坚持英文优先不动摇".**

The Chinese baseline below is kept as reference data only. It does **not** set the gate,
and it must not be allowed to drift the positioning: the whole reason English was chosen
is that the strongest free competitor is Chinese-first, so Chinese is the one market where
this product would be compared against *free* rather than against $49–199. A marginally
worse number in a market we are not entering is not a reason to move.

### What that leaves on the critical path

| item | state |
|---|---|
| recognition on clean English pages | ✅ 97.5% char F1, gate passed |
| recognition on real paper scans | ⚠️ **unmeasured** — skew, noise and JPEG mush are untested, and this is an upper bound until they are |
| **writing a searchable PDF** | 🔨 **in progress** — the milestone everything else depends on: hidden text layer placement, widths measured from the reader |
| **reading order** | ⚠️ **partial** — gutter reordering is in and worth +12.5 points (71% against reading-order truth), but full-width blocks below the columns are still hoisted to the front and three-column pages need more than one split |
| Column/layout handling | ⚠️ not started — likely PP-Structure (Apache-2.0) |

---

## Reference only: the Chinese baseline (93.7%)

The project was founded on an English-first position: the strongest free competitor,
Umi-OCR (MIT, offline, ~47k stars), is Chinese-first and barely visible in the English
world, so English was where the family could be something other than second best.

The operator's real documents, however, are Chinese. If the target market is actually
Chinese, that premise has to be re-examined, because the comparison stops being
"$9.99 versus a $49–199 incumbent" and becomes "$9.99 versus free, mature and offline".
One concrete thing worth checking first: Umi-OCR's platform coverage. If it is
Windows-only, Chinese-language macOS users are unserved — and the family ships macOS.

## Reading order: gutter detection works, and the right ruler matters

`reading_order.py` splits a two-column page at its gutter — the vertical band no
block crosses — using PP-OCR's own boxes. No new dependency.

Measured on the 20 English pages, two ground truths, one OCR run (cached):

| ground truth | raw detector order | gutter reordering |
|---|---|---|
| `pdftotext -layout` (visual page) | 82.8% | 58.9% |
| `pdftotext` (reading-order mode) | 58.5% | **71.0%** |

**The two rows are inverted, and that is the whole lesson.** `-layout` preserves the
visual page, so on a two-column paper it emits the columns woven together across each
visual line. The detector's raw order happens to match that, which made it look good and
made a *correct* reordering look like a regression. Judged against text that is itself
reconstructed in reading order, the reordering is worth **+12.5 points**.

**Rule: never judge reading order against `pdftotext -layout`.** Use the default mode.
`baseline.py --truth flow` is the default for this reason; `--truth layout` stays only to
demonstrate the trap.

**Not finished.** 71% is progress, not a solution:

- full-width blocks below the columns (figure captions, footers) are currently hoisted to
  the front, because a block that spans the gutter is ordered as if it were a heading
- gutter detection is a single global split; three-column pages and pages with an inset
  figure need more than one
- recognition itself is unchanged at 97.4–97.5% char F1 — reordering moves text, it does
  not read it
