//! Thin CLI over the writer library, kept for the measurement harnesses.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    let a: Vec<String> = std::env::args().collect();
    if a.len() > 1 && a[1] == "--pdf" {
        if a.len() != 6 && a.len() != 7 {
            eprintln!("usage: write-searchable --pdf <input.pdf> <pages.json> <out.pdf> <dpi> [replace]");
            std::process::exit(2);
        }
        let dpi: f64 = a[5].parse()?;
        let replace = a.get(6).map(|s| s == "replace").unwrap_or(false);
        return write_searchable::layer_on_existing(&a[2], &a[3], &a[4], dpi, replace);
    }
    if a.len() != 7 {
        eprintln!("usage: write-searchable <page.jpg> <boxes.json> <out.pdf> <width> <height> <dpi>");
        eprintln!("       write-searchable --pdf <input.pdf> <pages.json> <out.pdf> <dpi>");
        std::process::exit(2);
    }
    let w: i64 = a[4].parse()?;
    let h: i64 = a[5].parse()?;
    let dpi: f64 = a[6].parse()?;
    write_searchable::write_from_image(&a[1], &a[2], &a[3], w, h, dpi)
}
