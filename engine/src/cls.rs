//! The direction classifier: decides whether a cropped text bar is upside
//! down and rotates it back before recognition.

use anyhow::Result;

/// Returns the crop rotated 180 degrees when the classifier says so.
pub fn maybe_rotate(session: &mut ort::session::Session, crop: &image::RgbImage) -> Result<image::RgbImage> {
    let (img_h, img_w) = (48usize, 192usize);
    let (w, h) = (crop.width() as usize, crop.height() as usize);
    let ratio = w as f32 / h as f32;
    let resized_w = ((img_h as f32 * ratio).ceil() as usize).min(img_w);
    let resized = image::imageops::resize(
        crop,
        resized_w as u32,
        img_h as u32,
        image::imageops::FilterType::Triangle,
    );
    let mut input = vec![0f32; 3 * img_h * img_w];
    for y in 0..img_h {
        for x in 0..resized_w {
            let px = resized.get_pixel(x as u32, y as u32);
            let base = y * img_w + x;
            input[base] = (px[0] as f32 / 255.0 - 0.5) / 0.5;
            input[base + img_h * img_w] = (px[1] as f32 / 255.0 - 0.5) / 0.5;
            input[base + 2 * img_h * img_w] = (px[2] as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    let x = ort::value::Tensor::from_array(([1i64, 3, img_h as i64, img_w as i64], input))?;
    let outs = session.run(ort::inputs![x])?;
    let (_shape, probs) = outs[0].try_extract_tensor::<f32>()?;
    // Two logits in, softmax over them; label "180" is index 1.
    let e0 = probs[0].exp();
    let e1 = probs[1].exp();
    let p180 = e1 / (e0 + e1);
    if p180 > 0.9 {
        let mut out = crop.clone();
        image::imageops::rotate180_in_place(&mut out);
        Ok(out)
    } else {
        Ok(crop.clone())
    }
}
