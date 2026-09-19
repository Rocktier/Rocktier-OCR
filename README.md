# Rocktier OCR

Makes scans and screenshots searchable. Offline, private, small, one-time purchase.

**Status: not started.** No app code yet — and deliberately so. The project is gated on
an accuracy baseline (see *Gate* below), because accuracy decides whether it is worth
building at all.

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

**Threshold: 95% mean character accuracy** on clean scans. Below that, stop and
reconsider — the market has ABBYY at $199 and a free 47k-star alternative; "worse and
paid" is not a position.
