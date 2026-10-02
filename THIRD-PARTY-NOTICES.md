# Third-party notices

Rocktier OCR is released under the **MIT License**. The full text lives in
[LICENSE](LICENSE). There is **no warranty** for this program, to the extent
permitted by law.

This file lists the third-party components that ship inside the application or
are linked into it. Version numbers are deliberately not repeated here; the
authoritative dependency graph is `src-tauri/Cargo.lock` and `engine/Cargo.lock`
in the source tree.

---

## Bundled components

### PDFium — PDF rendering

| | |
|---|---|
| **Component** | PDFium, the prebuilt dynamic library (`libpdfium.dylib` / `pdfium.dll`) |
| **Licence** | BSD 3-Clause License |
| **Copyright** | Copyright 2013 The PDFium Authors. All rights reserved. |
| **Upstream source** | https://pdfium.googlesource.com/pdfium/ |
| **Binaries from** | https://github.com/bblanchon/pdfium-binaries (pinned upstream build `8057` in `scripts/fetch-pdfium.mjs`) |
| **Redistributed as** | `src-tauri/resources/pdfium-runtime/`, bundled with the app |

PDFium is loaded at runtime through the `pdfium-render` Rust binding (listed with
the crates below); no PDFium code is modified. Its BSD 3-Clause licence text:

> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are met:
>
> 1. Redistributions of source code must retain the above copyright notice, this
>    list of conditions and the following disclaimer.
> 2. Redistributions in binary form must reproduce the above copyright notice,
>    this list of conditions and the following disclaimer in the documentation
>    and/or other materials provided with the distribution.
> 3. Neither the name of the copyright holder nor the names of its contributors
>    may be used to endorse or promote products derived from this software
>    without specific prior written permission.
>
> THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
> AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
> IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
> DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
> FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
> DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
> SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
> CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
> OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
> OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

### PP-OCR ONNX recognition models

| | |
|---|---|
| **Component** | the bundled ONNX models `ch_PP-OCRv4_det_infer.onnx`, `ch_PP-OCRv4_rec_infer.onnx`, `ch_ppocr_mobile_v2.0_cls_infer.onnx` |
| **Licence** | Apache License 2.0 |
| **Copyright** | PaddlePaddle authors (PaddleOCR); model files as distributed by RapidOCR (`rapidocr_onnxruntime`, Apache-2.0) |
| **Upstream source** | https://github.com/PaddlePaddle/PaddleOCR · https://github.com/RapidAI/RapidOCR |
| **Redistributed as** | `src-tauri/resources/models/`, bundled with the app |

The detection / recognition / angle-classification models are the PaddleOCR
pre-trained models, used unmodified and loaded at runtime by the in-app engine.
The Apache License 2.0 text is published at
https://www.apache.org/licenses/LICENSE-2.0 and applies to each model file.

### Geist & Geist Mono — UI typefaces

| | |
|---|---|
| **Component** | Geist, Geist Mono variable webfonts (`ui/fonts/`) |
| **Licence** | SIL Open Font License 1.1 |
| **Copyright** | Vercel (https://vercel.com/font) |
| **Redistributed as** | `ui/fonts/Geist-Variable.woff2`, `ui/fonts/GeistMono-Variable.woff2`; the licence text ships alongside as `ui/fonts/OFL.txt` |

### ONNX Runtime — model inference runtime

| | |
|---|---|
| **Component** | ONNX Runtime, linked into the engine binary via `ort-sys` (binaries fetched at build time) |
| **Licence** | MIT License |
| **Copyright** | Copyright (c) Microsoft Corporation |
| **Upstream source** | https://github.com/microsoft/onnxruntime |

---

## Linked Rust crates

The application is a Tauri 2 app; the crates below are linked into the shipped
binaries. Unless a different licence is named, every crate is dual-licensed
under **MIT OR Apache-2.0** (the licence field of the exact shipped version is
recorded in the crate's own manifest; the resolved set lives in `Cargo.lock`).

**Via `src-tauri/Cargo.toml`** — `tauri`, `tauri-build`, `tauri-plugin-dialog`,
`tauri-plugin-clipboard-manager`, `tauri-plugin-opener`, `serde`, `serde_json`,
`image`, `anyhow`, `tempfile`, `pdfium-render` (MIT OR Apache-2.0).

**Via `engine/Cargo.toml`** — `ort` and `ort-sys` (MIT OR Apache-2.0, the Rust
bindings to ONNX Runtime above), `clipper2` (MIT OR Apache-2.0), plus `serde_json`,
`image`, `anyhow` already listed.

**Via `writer/Cargo.toml`** — `lopdf` (MIT License), `serde_json`.

`ocr-engine` and `write-searchable` are first-party crates in this repository
(`engine/`, `writer/`), not third-party components.

The complete transitive dependency set is pinned in `src-tauri/Cargo.lock` /
`engine/Cargo.lock`; every crate's licence can be verified from its manifest in
the upstream registry.

---

## Trademarks

Rocktier OCR is an **independent product**. It is not affiliated with, endorsed
by, or sponsored by the vendors of the components named above (Google, Microsoft,
PaddlePaddle, Vercel or others). All product names and trademarks are the
property of their respective owners.
