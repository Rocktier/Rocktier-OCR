//! Minimal searchable-PDF writer: one page image plus an invisible text layer.
//!
//! Takes a JPEG page and PP-OCR's line boxes, and writes a PDF whose text is
//! invisible (rendering mode 3) but selectable, positioned per word.
//!
//! Deliberately narrow. JPEG is embedded with /DCTDecode, so no image decoding
//! dependency is needed; the page size is passed in rather than parsed, because
//! the caller is the rasteriser and already knows it.
//!
//!     write-searchable <page.jpg> <boxes.json> <out.pdf> <width> <height> <dpi>
//!
//! boxes.json is the array PP-OCR returns: [{"box": [[x,y],...], "text": "..."}].
//! Word placement is estimated from word length rather than glyph metrics: the
//! text is invisible, so all that matters is where each word's hit-box lands.

use std::fs;

use lopdf::{dictionary, Document, Object, Stream};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 7 {
        eprintln!("usage: write-searchable <page.jpg> <boxes.json> <out.pdf> <width> <height> <dpi>");
        std::process::exit(2);
    }
    let (jpg, boxes_json, out) = (&a[1], &a[2], &a[3]);
    let w: i64 = a[4].parse()?;
    let h: i64 = a[5].parse()?;
    let dpi: f64 = a[6].parse()?;
    let scale = 72.0 / dpi;

    let jpeg = fs::read(jpg)?;
    let blocks: Vec<serde_json::Value> = serde_json::from_str(&fs::read_to_string(boxes_json)?)?;

    // Page geometry in points.
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
    // Every code unit that will be drawn, collected first because the font has to
    // describe all of them up front.
    let mut cids: Vec<u16> = Vec::new();
    for block in &blocks {
        if let Some(text) = block["text"].as_str() {
            for word in text.split_whitespace() {
                cids.extend(word.encode_utf16());
            }
        }
    }
    cids.sort_unstable();
    cids.dedup();

    // Nothing is ever painted - the text goes out in rendering mode 3 - so the font
    // only has to be right about two things: how to map a code back to Unicode, and
    // how wide each character is. Helvetica with WinAnsiEncoding got the widths right
    // but cannot hold a character outside Latin-1: an arrow went out as its UTF-8
    // bytes and came back as "a-t-'", one byte read as a character at a time. A Type0
    // font over Identity-H with an explicit ToUnicode map carries whatever the
    // detector returns.
    let mut tounicode = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    tounicode.push_str(&format!("{} beginbfchar\n", cids.len()));
    for c in &cids {
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
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "RocktierHiddenText",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![cid_font_id.into()],
        "ToUnicode" => tounicode_id,
    });

    let mut content = String::new();
    // The scan itself, drawn at full page size.
    content.push_str(&format!(
        "q\n{pw:.2} 0 0 {ph:.2} 0 0 cm\n/Im0 Do\nQ\nBT\n3 Tr\n"
    ));

    // The boxes this writer actually places, in order. The checker compares the
    // reader's geometry against this rather than against the input, so criterion 6
    // measures the write side instead of the detector.
    let mut placed: Vec<serde_json::Value> = Vec::new();
    // (y0, y1, right edge) of what has been drawn, used to nudge words apart.
    let mut extents: Vec<(f64, f64, f64)> = Vec::new();

    for block in &blocks {
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

        // Split into words and spread them across the line box by length, so a
        // selection lands on a word instead of swallowing the whole line.
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let _width = x1 - x0;
        content.push_str(&format!("/F0 {size:.2} Tf\n1 0 0 1 {x0:.2} {baseline:.2} Tm\n"));
        // Td is relative to the current line matrix, so each word advances by the
        // previous word's estimated width. Without this every word lands on top of
        // the first one, and the whole line selects as a single block.
        // Split the line box into words the way the reader will read it back:
        // proportionally to the words' real advances, then scaled as a whole so the
        // line ends exactly at the right edge of the box. Positions come from the
        // cumulative sum, never from an accumulated estimate, so nothing drifts.
        // Reserve the gaps between words first, then hand the width that is left to
        // the words themselves. The reader breaks words only when the gap exceeds
        // roughly a tenth of the font size, so a line squeezed to fit its box used to
        // lose its gaps and came back with neighbours fused - "Chordedit" and "Lu"
        // returned as "ChordeditLu". Reserving first keeps every gap above that
        // threshold, and unlike pushing words apart, which walks a whole line off the
        // page whenever a two-column box reaches across the gutter, this cannot
        // overflow: the words simply take what is left.
        // The gap keeps the width a space would naturally have and is not squeezed
        // with the words. poppler breaks words at roughly a tenth of the font size,
        // but Preview and Acrobat want more than that: measured against real output,
        // a gap of 0.15 em left a title reading "REFINEMENTISINHERENTLYEDITABLE" in
        // the viewer even though pdftotext saw every space. So the words absorb the
        // squeeze and the gaps do not.
        let min_gap = char_width(' ') / 1000.0 * size;
        let reserved = (words.len().saturating_sub(1)) as f64 * min_gap;
        let avail = _width - reserved;
        let units: f64 = words.iter().map(|w| word_units(w)).sum::<f64>();
        // Used only when the box is too narrow to hold the gaps at all.
        let squeeze = if units > 0.01 { _width / (units / 1000.0 * size) } else { 1.0 };

        // Only small overlaps are nudged apart. A large one means the detector put two
        // blocks on top of each other, which is a layout error and not something to
        // paper over: shoving a line clear of a box that reaches across the gutter
        // pushes the whole line off the page, which is how an earlier attempt here
        // dropped two thirds of the words on the page.
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
                "text": *word,
                "x0": start,
                "x1": start + advance,
                // Top-left origin, the same one pdftotext -bbox reports in, so the
                // checker can subtract the two without flipping anything.
                "y0": y0,
                "y1": y1,
            }));
            // Two-byte codes, which Identity-H reads as Unicode directly. A literal
            // string would go out as UTF-8 bytes and be read one byte per character,
            // which is how an arrow turned into three unrelated letters.
            let codes: String = word.encode_utf16().map(|u| format!("{u:04X}")).collect();
            content.push_str(&format!("1 0 0 1 {start:.2} {baseline:.2} Tm\n<{codes}> Tj\n"));            extents.push((y0, y1, start + advance));
            cursor = start + advance + min_gap;
        }
        content.push('\n');
    }
    content.push_str("ET\n");

    // Sidecar beside the PDF. Criterion 6 is a geometric comparison, and it cannot
    // be made without knowing where the words actually went.
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
