//! The searchable-PDF writer as a library: the CLI is a thin shell over this.
//! Everything that places, strips and encodes the text layer lives here so the
//! product binaries can call it in process.

//! Minimal searchable-PDF writer: an invisible, selectable text layer.
//!
//! Two modes, because the input decides what "keeping the document" means.
//!
//!     write-searchable <page.jpg> <boxes.json> <out.pdf> <width> <height> <dpi>
//!     write-searchable --pdf <input.pdf> <pages.json> <out.pdf> <dpi>
//!
//! The first builds a one-page PDF around a scanned image - what a screenshot
//! needs. The second opens an existing PDF and appends a text layer to every
//! page, which is the only honest way to handle a real document: pages, images,
//! metadata and bookmarks were never thrown away, so they cannot be lost. It is
//! also where "replace the text layer" has to start, since the old one is still
//! in front of us rather than already discarded.
//!
//! pages.json is an array with one entry per page, each the array PP-OCR returns:
//! [{"box": [[x,y],...], "text": "...", "words": [{"box": ..., "text": ...}]}].
//! `words` - the per-character boxes grouped on the recogniser's own spaces - is
//! preferred whenever it is present; without it the writer has to estimate.

use std::fs;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

/// Helvetica advance widths in 1/1000 em, for ASCII 32..=126.
///
/// Measured by probing this pipeline's own reader with a PDF that draws each
/// character once, rather than copied from the AFM spec: the extractor derives
/// glyph positions from its own metrics, so aligning to anything else leaves
/// drift. Cross-checked against the published values (A=667, W=944, i=222) - they
/// agree, which is the point of measuring rather than trusting.
const WIDTHS: [f64; 95] = [312.0, 278.0, 500.0, 556.0, 556.0, 889.0, 500.0, 500.0, 333.0, 333.0, 389.0, 584.0, 278.0, 333.0, 278.0, 278.0, 556.0, 556.0, 556.0, 556.0, 556.0, 556.0, 556.0, 556.0, 556.0, 556.0, 278.0, 278.0, 500.0, 584.0, 500.0, 556.0, 1015.0, 667.0, 667.0, 722.0, 722.0, 667.0, 611.0, 778.0, 722.0, 278.0, 500.0, 667.0, 556.0, 833.0, 722.0, 778.0, 667.0, 778.0, 722.0, 667.0, 611.0, 722.0, 667.0, 944.0, 667.0, 667.0, 611.0, 278.0, 278.0, 278.0, 469.0, 556.0, 333.0, 556.0, 556.0, 500.0, 556.0, 556.0, 278.0, 556.0, 556.0, 222.0, 222.0, 500.0, 222.0, 833.0, 556.0, 556.0, 556.0, 556.0, 333.0, 500.0, 278.0, 556.0, 500.0, 722.0, 500.0, 500.0, 500.0, 334.0, 260.0, 334.0, 584.0];

fn word_units(word: &str) -> f64 {
    word.chars().map(char_width).sum()
}

fn char_width(ch: char) -> f64 {
    let c = ch as u32;
    if (32..=126).contains(&c) { WIDTHS[(c - 32) as usize] } else { 500.0 }
}

/// Every code unit the text will use. The font has to describe all of them up
/// front, so this is collected before anything is drawn.
fn collect_cids(pages: &[Vec<serde_json::Value>]) -> Vec<u16> {
    let mut cids: Vec<u16> = Vec::new();
    for blocks in pages {
        for block in blocks {
            if let Some(text) = block["text"].as_str() {
                for word in text.split_whitespace() {
                    cids.extend(word.encode_utf16());
                }
            }
        }
    }
    cids.sort_unstable();
    cids.dedup();
    cids
}

/// The font the invisible text is drawn with.
///
/// Nothing is ever painted - the text goes out in rendering mode 3 - so the font
/// only has to be right about two things: how a code maps back to Unicode, and
/// how wide each character is. Helvetica with WinAnsiEncoding got the widths right
/// but cannot hold a character outside Latin-1: an arrow went out as its UTF-8
/// bytes and came back as "a-t-'", one byte read as a character at a time. A Type0
/// font over Identity-H with an explicit ToUnicode map carries whatever the
/// detector returns.
fn add_font(doc: &mut Document, cids: &[u16]) -> ObjectId {
    let mut tounicode = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    tounicode.push_str(&format!("{} beginbfchar\n", cids.len()));
    for c in cids {
        tounicode.push_str(&format!("<{c:04X}> <{c:04X}>\n"));
    }
    tounicode.push_str(
        "endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
    );
    let tounicode_id = doc.add_object(Stream::new(dictionary! {}, tounicode.into_bytes()));

    // The widths this writer advances by, so words cannot overlap into each other.
    // 1/1000 em, which is what /W is defined in.
    let widths: Vec<Object> = cids
        .iter()
        .flat_map(|c| {
            let w = match char::from_u32(*c as u32) {
                Some(ch) => char_width(ch) as i64,
                None => 500,
            };
            [Object::Integer(*c as i64), Object::Array(vec![Object::Integer(w)])]
        })
        .collect();

    let descriptor_id = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "RocktierHiddenText",
        "Flags" => 4,
        "FontBBox" => vec![0.into(), (-200).into(), 1000.into(), 1000.into()],
        "ItalicAngle" => 0,
        "Ascent" => 800,
        "Descent" => -200,
        "CapHeight" => 700,
        "StemV" => 80,
    });
    // No /FontFile2: nothing is ever rasterised from this font, only mapped back out.
    let cid_font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "RocktierHiddenText",
        "CIDSystemInfo" => dictionary! {
            "Registry" => "Adobe",
            "Ordering" => "Identity",
            "Supplement" => 0,
        },
        "DW" => 1000,
        "W" => Object::Array(widths),
        "FontDescriptor" => descriptor_id,
    });
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "RocktierHiddenText",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![cid_font_id.into()],
        "ToUnicode" => tounicode_id,
    })
}

/// The text layer for one page: the drawing operations, and where every word went.
///
/// The second return value is the sidecar. Criterion 6 is a geometric comparison
/// and cannot be made without knowing where the words actually landed, so this is
/// recorded as they are placed rather than reconstructed later.
fn text_layer(
    blocks: &[serde_json::Value],
    scale: f64,
    ph: f64,
    page: usize,
) -> Result<(String, Vec<serde_json::Value>), Box<dyn std::error::Error>> {
    let mut content = String::from("BT\n3 Tr\n");
    let mut placed: Vec<serde_json::Value> = Vec::new();
    // (y0, y1, right edge) of what has been drawn, used to nudge words apart.
    let mut extents: Vec<(f64, f64, f64)> = Vec::new();
    // (x0, x1, bottom edge) of vertical runs, same idea along the other axis.
    let mut v_extents: Vec<(f64, f64, f64)> = Vec::new();

    for block in blocks {
        let text = block["text"].as_str().unwrap_or("");
        if text.is_empty() {
            continue;
        }
        let pts = block["box"].as_array().ok_or("box must be an array")?;
        let xs: Vec<f64> = pts.iter().map(|p| p[0].as_f64().unwrap_or(0.0) * scale).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p[1].as_f64().unwrap_or(0.0) * scale).collect();
        let x0 = xs.iter().cloned().fold(f64::MAX, f64::min);
        let x1 = xs.iter().cloned().fold(f64::MIN, f64::max);
        let y0 = ys.iter().cloned().fold(f64::MAX, f64::min);
        let y1 = ys.iter().cloned().fold(f64::MIN, f64::max);

        // PDF's origin is bottom-left; the detector's is top-left.
        let baseline = ph - y1;
        let size = (y1 - y0).max(1.0);

        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        // Reading direction. A scan saved sideways (no /Rotate, image stored
        // turned) yields tall narrow line boxes - the engine straightens each
        // line before recognising, so the text comes back perfect and the
        // geometry sideways. Drawing such a line as horizontal text crushes it
        // into the sliver's width and the extractor drops the overlapped
        // glyphs, so the direction is honoured with a rotated text matrix.
        //
        // The quad itself cannot be asked for the direction: measured on a
        // CamScanner page, vertical lines come back as axis-aligned rectangles
        // in raster order (tl, tr, br, bl), so p0 -> p1 is the short edge. The
        // bbox shape decides instead, with a ratio and a length guard so a
        // narrow glyph such as a lone "1" is never mistaken for a column.
        // Vertical CJK reads top to bottom; glyphs grow toward +x off the
        // baseline at the strip's left edge.
        let vertical = (y1 - y0) > (x1 - x0) * 2.0 && text.chars().count() > 1;
        let (dirx, diry, upx, upy) = if vertical {
            (0.0, -1.0, 1.0, 0.0)
        } else {
            (1.0, 0.0, 0.0, 1.0)
        };

        // Prefer the word boxes the engine measured. They carry each word's real
        // width and the real gap that follows it, so nothing has to be estimated.
        // Everything after this branch is the estimate used when they are absent -
        // spreading words across the line's box by length - and estimating is what
        // let neighbours come back fused, "Chordedit" and "Lu" as one word.
        if let Some(measured) = block["words"].as_array() {
            if !measured.is_empty() {
                for w in measured {
                    let wt = match w["text"].as_str() {
                        Some(t) => t,
                        None => continue,
                    };
                    if wt.is_empty() {
                        continue;
                    }
                    let wp = match w["box"].as_array() {
                        Some(p) => p,
                        None => continue,
                    };
                    let wxs: Vec<f64> =
                        wp.iter().map(|p| p[0].as_f64().unwrap_or(0.0) * scale).collect();
                    let wys: Vec<f64> =
                        wp.iter().map(|p| p[1].as_f64().unwrap_or(0.0) * scale).collect();
                    let wx0 = wxs.iter().cloned().fold(f64::MAX, f64::min);
                    let wx1 = wxs.iter().cloned().fold(f64::MIN, f64::max);
                    let wy0 = wys.iter().cloned().fold(f64::MAX, f64::min);
                    let wy1 = wys.iter().cloned().fold(f64::MIN, f64::max);
                    if vertical {
                        // A sideways line: the box's width is the line's
                        // thickness, its height the run the text travels. The
                        // baseline sits on the side the glyphs grow from, and
                        // reading starts where reading starts.
                        //
                        // Overlapping strips are real: the detector's boxes for
                        // stacked header cells share the same narrow column, and
                        // PDFium silently drops invisible text that overlaps
                        // other invisible text (poppler keeps it, which is how
                        // this passed the first real-scan run). So vertical
                        // words get the same nudging the horizontal ones do,
                        // along the run instead of across it.
                        let thick = (wx1 - wx0).max(1.0);
                        let mut run_top = wy0;
                        let run = wy1 - wy0;
                        let min_gap = char_width(' ') / 1000.0 * thick;
                        let max_nudge = 2.0 * thick;
                        for &(vx0, vx1, vbottom) in &v_extents {
                            if wx0 < vx1 - 0.01 && wx1 > vx0 + 0.01 && run_top < vbottom {
                                let need = vbottom + min_gap - run_top;
                                if need > 0.0 && need <= max_nudge {
                                    run_top = vbottom + min_gap;
                                }
                            }
                        }
                        let natural = word_units(wt) / 1000.0 * thick;
                        let stretch = if natural > 0.01 { run / natural } else { 1.0 };
                        let ex = if upx > 0.0 { wx0 } else { wx1 };
                        let ey = if diry < 0.0 { ph - run_top } else { ph - (run_top + run) };
                        content.push_str(&format!("/F0 {thick:.2} Tf\n{:.3} Tz\n", stretch * 100.0));
                        placed.push(serde_json::json!({
                            "page": page,
                            "text": wt,
                            "x0": wx0,
                            "x1": wx1,
                            "y0": run_top,
                            "y1": run_top + run,
                        }));
                        let codes: String = wt.encode_utf16().map(|u| format!("{u:04X}")).collect();
                        content.push_str(&format!(
                            "{dirx:.4} {diry:.4} {upx:.4} {upy:.4} {ex:.2} {ey:.2} Tm\n<{codes}> Tj\n"));
                        v_extents.push((wx0, wx1, run_top + run));
                        continue;
                    }

                    let wsize = (wy1 - wy0).max(1.0);

                    // Render the word at exactly the width the engine measured.
                    let natural = word_units(wt) / 1000.0 * wsize;
                    let stretch = if natural > 0.01 { (wx1 - wx0) / natural } else { 1.0 };

                    // The measured width is kept, but a measured gap that is too
                    // small is not: the reader needs roughly a tenth of the font
                    // size to see a break, and viewers want more. Nudging is capped
                    // because a genuinely large overlap is a layout error, and
                    // shoving a word clear of one walks the rest of the line off
                    // the page.
                    let min_gap = char_width(' ') / 1000.0 * wsize;
                    let max_nudge = 1.2 * wsize;
                    let mut start = wx0;
                    for &(py0, py1, px1) in &extents {
                        if wy0 < py1 - 0.01 && wy1 > py0 + 0.01 {
                            let need = px1 + min_gap - start;
                            if need > 0.0 && need <= max_nudge {
                                start = px1 + min_gap;
                            }
                        }
                    }
                    let end = start + (wx1 - wx0);

                    content.push_str(&format!("/F0 {wsize:.2} Tf\n{:.3} Tz\n", stretch * 100.0));
                    placed.push(serde_json::json!({
                        "page": page,
                        "text": wt,
                        "x0": start,
                        "x1": end,
                        // Top-left origin, the one pdftotext -bbox reports in, so
                        // the checker can subtract the two without flipping.
                        "y0": wy0,
                        "y1": wy1,
                    }));
                    let codes: String = wt.encode_utf16().map(|u| format!("{u:04X}")).collect();
                    content.push_str(&format!(
                        "1 0 0 1 {start:.2} {:.2} Tm\n<{codes}> Tj\n",
                        ph - wy1));
                    extents.push((wy0, wy1, end));
                }
                content.push('\n');
                continue;
            }
        }

        if vertical {
            // No measured word boxes on a sideways line: the same approximation
            // the horizontal estimate path makes, one run spread across the box,
            // just along the reading direction. Nudged like the measured path.
            let thick = (x1 - x0).max(1.0);
            let run = y1 - y0;
            let mut run_top = y0;
            let min_gap = char_width(' ') / 1000.0 * thick;
            let max_nudge = 2.0 * thick;
            for &(vx0, vx1, vbottom) in &v_extents {
                if x0 < vx1 - 0.01 && x1 > vx0 + 0.01 && run_top < vbottom {
                    let need = vbottom + min_gap - run_top;
                    if need > 0.0 && need <= max_nudge {
                        run_top = vbottom + min_gap;
                    }
                }
            }
            let joined = words.join(" ");
            let natural = word_units(&joined) / 1000.0 * thick;
            let stretch = if natural > 0.01 { run / natural } else { 1.0 };
            let ex = if upx > 0.0 { x0 } else { x1 };
            let ey = if diry < 0.0 { ph - run_top } else { ph - (run_top + run) };
            let codes: String = joined.encode_utf16().map(|u| format!("{u:04X}")).collect();
            content.push_str(&format!(
                "/F0 {thick:.2} Tf\n{:.3} Tz\n{dirx:.4} {diry:.4} {upx:.4} {upy:.4} {ex:.2} {ey:.2} Tm\n<{codes}> Tj\n",
                stretch * 100.0));
            placed.push(serde_json::json!({
                "page": page,
                "text": joined,
                "x0": x0,
                "x1": x1,
                "y0": run_top,
                "y1": run_top + run,
            }));
            v_extents.push((x0, x1, run_top + run));
            content.push('\n');
            continue;
        }

        let _width = x1 - x0;
        content.push_str(&format!("/F0 {size:.2} Tf\n1 0 0 1 {x0:.2} {baseline:.2} Tm\n"));
        // The gap keeps the width a space would naturally have and is not squeezed
        // with the words. poppler breaks words at roughly a tenth of the font size,
        // but the readers people open PDFs in want more: at 0.15 em a title came
        // back as "REFINEMENTISINHERENTLYEDITABLE" in Edge while pdftotext saw
        // every space. So the words absorb the squeeze and the gaps do not.
        let min_gap = char_width(' ') / 1000.0 * size;
        let reserved = (words.len().saturating_sub(1)) as f64 * min_gap;
        let avail = _width - reserved;
        let units: f64 = words.iter().map(|w| word_units(w)).sum::<f64>();
        // Used only when the box is too narrow to hold the gaps at all.
        let squeeze = if units > 0.01 { _width / (units / 1000.0 * size) } else { 1.0 };

        // Only small overlaps are nudged apart. A large one means the detector put
        // two blocks on top of each other, which is a layout error and not
        // something to paper over: shoving a line clear of a box that reaches
        // across the gutter pushes the whole line off the page.
        let max_nudge = 0.5 * size;
        let mut cursor = x0;
        for word in words.iter() {
            let mut start = cursor;
            for &(py0, py1, px1) in &extents {
                if y0 < py1 - 0.01 && y1 > py0 + 0.01 {
                    let need = px1 + min_gap - start;
                    if need > 0.0 && need <= max_nudge {
                        start = px1 + min_gap;
                    }
                }
            }
            let natural_w = word_units(word) / 1000.0 * size;
            let (advance, stretch) = if avail > 0.0 && units > 0.01 {
                let share = avail * word_units(word) / units;
                (share, if natural_w > 0.01 { share / natural_w } else { 1.0 })
            } else {
                (natural_w * squeeze, squeeze)
            };
            // Tz makes each word render at exactly the width it was given. The font
            // size stays at the line height, so the hit-box keeps the right height.
            content.push_str(&format!("{:.3} Tz\n", stretch * 100.0));
            placed.push(serde_json::json!({
                "page": page,
                "text": *word,
                "x0": start,
                "x1": start + advance,
                "y0": y0,
                "y1": y1,
            }));
            // Two-byte codes, which Identity-H reads as Unicode directly. A literal
            // string would go out as UTF-8 bytes and be read one byte per
            // character, which is how an arrow turned into three letters.
            let codes: String = word.encode_utf16().map(|u| format!("{u:04X}")).collect();
            content.push_str(
                &format!("1 0 0 1 {start:.2} {baseline:.2} Tm\n<{codes}> Tj\n"));
            extents.push((y0, y1, start + advance));
            cursor = start + advance + min_gap;
        }
        content.push('\n');
    }
    content.push_str("ET\n");
    Ok((content, placed))
}

/// Make /F0 reachable from one page, leaving everything else in /Resources alone.
fn ensure_font(doc: &mut Document, page_id: ObjectId, font_id: ObjectId) -> Result<(), Box<dyn std::error::Error>> {
    enum Res { Ref(ObjectId), Inline(Dictionary), Missing }
    let res = {
        let dict = doc.get_dictionary(page_id)?;
        match dict.get(b"Resources") {
            Ok(Object::Reference(id)) => Res::Ref(*id),
            Ok(Object::Dictionary(d)) => Res::Inline(d.clone()),
            _ => Res::Missing,
        }
    };
    // /Resources has to be indirect before its /Font dictionary can be edited.
    let res_id = match res {
        Res::Ref(id) => id,
        Res::Inline(d) => {
            let rid = doc.add_object(Object::Dictionary(d));
            doc.get_object_mut(page_id)
                .and_then(Object::as_dict_mut)?
                .set("Resources", Object::Reference(rid));
            rid
        }
        Res::Missing => {
            let rid = doc.add_object(Object::Dictionary(dictionary! {}));
            doc.get_object_mut(page_id)
                .and_then(Object::as_dict_mut)?
                .set("Resources", Object::Reference(rid));
            rid
        }
    };

    enum Fnt { Ref(ObjectId), Inline(Dictionary), Missing }
    let fnt = {
        let dict = doc.get_object(res_id).and_then(Object::as_dict)?;
        match dict.get(b"Font") {
            Ok(Object::Reference(id)) => Fnt::Ref(*id),
            Ok(Object::Dictionary(d)) => Fnt::Inline(d.clone()),
            _ => Fnt::Missing,
        }
    };
    match fnt {
        Fnt::Ref(fid) => {
            doc.get_object_mut(fid)
                .and_then(Object::as_dict_mut)?
                .set("F0", Object::Reference(font_id));
        }
        Fnt::Inline(mut d) => {
            d.set("F0", Object::Reference(font_id));
            doc.get_object_mut(res_id)
                .and_then(Object::as_dict_mut)?
                .set("Font", Object::Dictionary(d));
        }
        Fnt::Missing => {
            doc.get_object_mut(res_id).and_then(Object::as_dict_mut)?.set(
                "Font",
                Object::Dictionary(dictionary! { "F0" => Object::Reference(font_id) }),
            );
        }
    }
    Ok(())
}

/// Remove the text this page already had, keeping everything else.
///
/// This is what makes the result a replacement rather than a second layer. The
/// text-showing operators are simply dropped; the images and vector work around
/// them are untouched, so the page still looks identical.
///
/// The page's own content stream is not the only place text can live. Form
/// XObjects carry it too, and scanner-produced "searchable PDFs" routinely wrap
/// their OCR layer in one - leaving those alone would stack two layers exactly
/// where a replacement was asked for. So the strip recurses through the page's
/// XObjects and through each form's own resources, visiting every object at
/// most once. Annotations are a separate tree and are still not touched; scans
/// rarely carry text-bearing ones.
fn strip_text(doc: &mut Document, page_id: ObjectId) -> Result<(), Box<dyn std::error::Error>> {
    let content = doc.get_and_decode_page_content(page_id)?;
    let kept: Vec<Operation> = content
        .operations
        .into_iter()
        .filter(|op| !is_text_showing(op))
        .collect();
    let bytes = Content { operations: kept }.encode()?;
    doc.change_page_content(page_id, bytes)?;

    let mut visited: Vec<ObjectId> = Vec::new();
    let entries = page_xobject_entries(doc, page_id);
    strip_form_entries(doc, entries, &mut visited)
}

/// The four operators that put glyphs on the page.
fn is_text_showing(op: &Operation) -> bool {
    matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\"")
}

/// Every XObject the page can name, collected up front: the resource dictionary
/// is borrowed from the document, and stripping mutates the document.
fn page_xobject_entries(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    let xobj = match doc.get_page_resources(page_id) {
        Ok((Some(res), _)) => res.get(b"XObject").ok().cloned(),
        _ => None,
    };
    match xobj {
        Some(Object::Dictionary(d)) => d.iter().map(|(_, v)| v.clone()).collect(),
        Some(Object::Reference(r)) => doc
            .get_object(r)
            .and_then(Object::as_dict)
            .map(|d| d.iter().map(|(_, v)| v.clone()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn strip_form_entries(
    doc: &mut Document,
    entries: Vec<Object>,
    visited: &mut Vec<ObjectId>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in entries {
        let id = match entry {
            Object::Reference(id) => id,
            // A stream stored directly in the resource dictionary has no id to
            // memoise and no producer in this corpus writes one; skipping beats
            // half-handling it.
            _ => continue,
        };
        if visited.contains(&id) {
            continue;
        }
        visited.push(id);

        let stream = match doc.get_object(id).ok().and_then(|o| o.as_stream().ok()) {
            Some(s) if s.dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Form") => s,
            _ => continue, // images and anything else: not text carriers to strip
        };
        let plain = stream.get_plain_content()?;
        if let Ok(content) = Content::decode(&plain) {
            let kept: Vec<Operation> = content
                .operations
                .into_iter()
                .filter(|op| !is_text_showing(op))
                .collect();
            let bytes = Content { operations: kept }.encode()?;
            if let Object::Stream(st) = doc.get_object_mut(id)? {
                st.set_plain_content(bytes);
            }
        }

        // A form may draw further forms through its own /Resources.
        let nested = doc
            .get_object(id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.dict.get(b"Resources").ok().cloned())
            .and_then(|res| match res {
                Object::Dictionary(d) => d.get(b"XObject").ok().cloned(),
                Object::Reference(r) => doc
                    .get_object(r)
                    .ok()
                    .and_then(|o| o.as_dict().ok().cloned())
                    .and_then(|d| d.get(b"XObject").ok().cloned()),
                _ => None,
            })
            .map(|xobj| match xobj {
                Object::Dictionary(d) => d.iter().map(|(_, v)| v.clone()).collect(),
                Object::Reference(r) => doc
                    .get_object(r)
                    .and_then(Object::as_dict)
                    .map(|d| d.iter().map(|(_, v)| v.clone()).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        strip_form_entries(doc, nested, visited)?;
    }
    Ok(())
}

/// Width and height of a page in points, from its MediaBox.
fn page_size(doc: &Document, page_id: ObjectId) -> (f64, f64) {
    if let Ok(dict) = doc.get_dictionary(page_id) {
        if let Ok(Object::Array(a)) = dict.get(b"MediaBox") {
            if a.len() == 4 {
                // Entries may be Integer or Real; as_float takes both.
                let x0 = a[0].as_float().unwrap_or(0.0) as f64;
                let y0 = a[1].as_float().unwrap_or(0.0) as f64;
                let x1 = a[2].as_float().unwrap_or(612.0) as f64;
                let y1 = a[3].as_float().unwrap_or(792.0) as f64;
                let (w, h) = (x1 - x0, y1 - y0);
                if w > 1.0 && h > 1.0 {
                    return (w, h);
                }
            }
        }
    }
    (612.0, 792.0)
}

/// Does this page's visible content come from an image covering most of it?
///
/// Only such a page may have its text stripped. On a scan the ink lives in the
/// image and the text is somebody else's OCR layer, so taking the text out
/// leaves the page intact. On a page born digital the text IS the content:
/// removing it erases the page - which is exactly what replace did to this
/// corpus, and what the visual criterion caught.
///
/// The test tracks the CTM through the content stream and adds up the area
/// every image is drawn over. A page whose images cover most of it is treated
/// as a scan; anything else keeps its text.
pub fn page_is_scan(doc: &Document, page_id: ObjectId) -> bool {
    let content = match doc.get_and_decode_page_content(page_id) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let (pw, ph) = page_size(doc, page_id);
    if pw <= 1.0 || ph <= 1.0 {
        return false;
    }
    let page_area = pw * ph;

    // Names of XObjects that are images. Resources may be inherited, so ask the
    // document rather than reading the page's own dictionary.
    let xobjects = doc.get_page_resources(page_id).ok().and_then(|(res, _)| {
        res.and_then(|d| d.get(b"XObject").ok().cloned())
            .and_then(|o| match o {
                Object::Dictionary(d) => Some(d),
                Object::Reference(id) => doc
                    .get_object(id)
                    .and_then(Object::as_dict)
                    .ok()
                    .map(|d| d.clone()),
                _ => None,
            })
    });

    let mut ctm = [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut stack: Vec<[f64; 6]> = Vec::new();
    let mut covered = 0.0f64;
    for op in &content.operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            "cm" => {
                let mut mm = [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0];
                for (i, o) in op.operands.iter().take(6).enumerate() {
                    mm[i] = o.as_float().unwrap_or(0.0) as f64;
                }
                // PDF concatenates as cm x CTM.
                ctm = [
                    mm[0] * ctm[0] + mm[1] * ctm[2],
                    mm[0] * ctm[1] + mm[1] * ctm[3],
                    mm[2] * ctm[0] + mm[3] * ctm[2],
                    mm[2] * ctm[1] + mm[3] * ctm[3],
                    mm[4] * ctm[0] + mm[5] * ctm[2] + ctm[4],
                    mm[4] * ctm[1] + mm[5] * ctm[3] + ctm[5],
                ];
            }
            "Do" => {
                let name = match op.operands.first().and_then(|o| o.as_name().ok()) {
                    Some(n) => n,
                    None => continue,
                };
                let is_image = xobjects
                    .as_ref()
                    .and_then(|d| d.get(name).ok())
                    .map(|o| match o {
                        Object::Stream(st) => st
                            .dict
                            .get(b"Subtype")
                            .ok()
                            .and_then(|s| s.as_name().ok())
                            .map(|n| n == b"Image")
                            .unwrap_or(false),
                        Object::Reference(id) => doc
                            .get_object(*id)
                            .ok()
                            .and_then(|o| o.as_stream().ok())
                            .and_then(|st| {
                                st.dict
                                    .get(b"Subtype")
                                    .ok()
                                    .and_then(|s| s.as_name().ok())
                                    .map(|n| n == b"Image")
                            })
                            .unwrap_or(false),
                        _ => false,
                    })
                    .unwrap_or(false);
                if is_image {
                    // A unit square drawn through this matrix has area |det|.
                    covered += (ctm[0] * ctm[3] - ctm[1] * ctm[2]).abs();
                }
            }
            _ => {}
        }
    }
    covered / page_area >= 0.7
}

/// Height of a page in points, from its MediaBox.
fn page_height(doc: &Document, page_id: ObjectId) -> f64 {
    if let Ok(dict) = doc.get_dictionary(page_id) {
        if let Ok(Object::Array(a)) = dict.get(b"MediaBox") {
            if a.len() == 4 {
                // MediaBox entries may be Integer or Real; as_float takes both.
                let y0 = a[1].as_float().unwrap_or(0.0) as f64;
                let y1 = a[3].as_float().unwrap_or(792.0) as f64;
                let h = y1 - y0;
                if h > 1.0 {
                    return h;
                }
            }
        }
    }
    792.0
}

/// Append a text layer to every page of an existing PDF.
pub fn layer_on_existing(
    input: &str,
    pages_json: &str,
    out: &str,
    dpi: f64,
    replace: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::load(input)?;
    doc.decompress();
    let pages: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(&fs::read_to_string(pages_json)?)?;
    let scale = 72.0 / dpi;

    let cids = collect_cids(&pages);
    let font_id = add_font(&mut doc, &cids);

    let ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut all_placed: Vec<serde_json::Value> = Vec::new();
    for (i, page_id) in ids.iter().enumerate() {
        let ph = page_height(&doc, *page_id);
        let blocks = pages.get(i).cloned().unwrap_or_default();
        let (content, mut placed) =
            text_layer(&blocks, scale, ph, i + 1)?;
        all_placed.append(&mut placed);
        // Only a page we actually recognised gets its old text taken out. Stripping
        // one we failed on would destroy the only text it has.
        // Strip only what the engine recognised, and only where the page is a
        // scan. On a page born digital the text is the content, and stripping it
        // erases the page - which is what replace did to this corpus before the
        // visual criterion caught it.
        if replace && !blocks.is_empty() && page_is_scan(&doc, *page_id) {
            strip_text(&mut doc, *page_id)?;
        }
        ensure_font(&mut doc, *page_id, font_id)?;
        doc.add_page_contents(*page_id, content.into_bytes())?;
    }

    // Sidecar beside the PDF. Criterion 6 is a geometric comparison, and it
    // cannot be made without knowing where the words actually went.
    fs::write(format!("{out}.boxes.json"), serde_json::to_vec(&all_placed)?)?;
    doc.compress();
    doc.save(out)?;
    Ok(())
}

/// Build a one-page PDF around a scanned image.
pub fn write_from_image(
    jpg: &str,
    boxes_json: &str,
    out: &str,
    w: i64,
    h: i64,
    dpi: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let jpeg = fs::read(jpg)?;
    let blocks: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(boxes_json)?)?;
    let scale = 72.0 / dpi;
    let pw = w as f64 * scale;
    let ph = h as f64 * scale;

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w,
            "Height" => h,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg,
    ));

    let cids = collect_cids(&[blocks.clone()]);
    let font_id = add_font(&mut doc, &cids);

    let (layer, placed) = text_layer(&blocks, scale, ph, 1)?;
    let mut content =
        format!("q\n{pw:.2} 0 0 {ph:.2} 0 0 cm\n/Im0 Do\nQ\n");
    content.push_str(&layer);

    fs::write(format!("{out}.boxes.json"), serde_json::to_vec(&placed)?)?;

    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), pw.into(), ph.into()],
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Im0" => image_id },
            "Font" => dictionary! { "F0" => font_id },
        },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.compress();
    doc.save(out)?;
    Ok(())
}

