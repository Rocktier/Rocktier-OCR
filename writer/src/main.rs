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
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

    let mut content = String::new();
    // The scan itself, drawn at full page size.
    content.push_str(&format!(
        "q\n{pw:.2} 0 0 {ph:.2} 0 0 cm\n/Im0 Do\nQ\nBT\n3 Tr\n"
    ));

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
        let total: usize = words.iter().map(|w| w.chars().count()).sum();
        let width = x1 - x0;
        content.push_str(&format!("/F0 {size:.2} Tf\n1 0 0 1 {x0:.2} {baseline:.2} Tm\n"));
        // Td is relative to the current line matrix, so each word advances by the
        // previous word's estimated width. Without this every word lands on top of
        // the first one, and the whole line selects as a single block.
        let mut advance = 0.0;
        for word in words {
            if advance != 0.0 {
                content.push_str(&format!("{advance:.2} 0 Td\n"));
            }
            let escaped = word
                .replace('\\', r"\\")
                .replace('(', r"\(")
                .replace(')', r"\)");
            content.push_str(&format!("({escaped}) Tj\n"));
            advance = width * (word.chars().count() as f64 / total.max(1) as f64);
        }
        content.push('\n');
    }
    content.push_str("ET\n");

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
