//! The recogniser: PP-OCRv4 rec model, CTC decode with the character table
//! read from the model's own metadata - the Python engine reads the same
//! table from the same place, so there is no external dictionary to ship.

use anyhow::Result;

#[derive(Clone)]
pub struct RecResult {
    pub text: String,
    pub score: f32,
    /// Decoded (non-blank) network columns, for word-box mapping.
    pub selection: Vec<usize>,
    /// Total network steps for this line.
    pub steps: usize,
}

pub struct RecModel {
    session: ort::session::Session,
    /// The decoder alphabet: the metadata list with " " appended and "blank"
    /// inserted at index zero, exactly as `CTCLabelDecode.get_character` does.
    characters: Vec<String>,
}

impl RecModel {
    pub fn load(model_path: &std::path::Path) -> Result<Self> {
        let mut session = ort::session::Session::builder()?.commit_from_file(model_path)?;
        // The alphabet rides in the model metadata, key "character".
        let raw = session
            .metadata()?
            .custom("character")
            .ok_or_else(|| anyhow::anyhow!("rec model carries no character metadata"))?;
        let mut characters: Vec<String> = raw.split('\n').map(|s| s.to_string()).collect();
        if characters.last().map(|s| s.is_empty()).unwrap_or(false) {
            characters.pop();
        }
        characters.push(" ".to_string());
        characters.insert(0, "blank".to_string());
        Ok(Self { session, characters })
    }

    /// `resize_norm_img`: scale to height 48, pad right to the batch width,
    /// normalise to [-1, 1]. Padding stays zero, which is mid-grey here too.
    fn resize_norm(&self, img: &image::RgbImage, max_wh_ratio: f32) -> (Vec<f32>, usize) {
        let img_h = 48usize;
        let img_w = (img_h as f32 * max_wh_ratio) as usize;
        let (w, h) = (img.width() as usize, img.height() as usize);
        let ratio = w as f32 / h as f32;
        let resized_w = ((img_h as f32 * ratio).ceil() as usize).min(img_w);
        let resized = image::imageops::resize(
            img,
            resized_w as u32,
            img_h as u32,
            image::imageops::FilterType::Triangle,
        );
        let mut out = vec![0f32; 3 * img_h * img_w];
        for y in 0..img_h {
            for x in 0..resized_w {
                let px = resized.get_pixel(x as u32, y as u32);
                let base = y * img_w + x;
                out[base] = (px[0] as f32 / 255.0 - 0.5) / 0.5;
                out[base + img_h * img_w] = (px[1] as f32 / 255.0 - 0.5) / 0.5;
                out[base + 2 * img_h * img_w] = (px[2] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        (out, img_w)
    }

    /// Recognise a list of cropped text bars. Batching follows the Python:
    /// sorted by aspect ratio, six at a time, tensor width set by the widest
    /// ratio in the batch - that padding is part of the model's contract.
    pub fn recognize(&mut self, crops: &[image::RgbImage]) -> Result<Vec<RecResult>> {
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by(|&a, &b| {
            let ra = crops[a].width() as f32 / crops[a].height() as f32;
            let rb = crops[b].width() as f32 / crops[b].height() as f32;
            ra.partial_cmp(&rb).unwrap()
        });
        let mut results = vec![
            RecResult { text: String::new(), score: 0.0, selection: Vec::new(), steps: 0 };
            crops.len()
        ];
        for batch in order.chunks(6) {
            let mut max_wh_ratio = 320f32 / 48.0;
            for &i in batch {
                let c = &crops[i];
                max_wh_ratio = max_wh_ratio.max(c.width() as f32 / c.height() as f32);
            }
            let mut tensors = Vec::with_capacity(batch.len());
            let mut width = 0usize;
            for &i in batch {
                let (data, w) = self.resize_norm(&crops[i], max_wh_ratio);
                width = w;
                tensors.push(data);
            }
            let h = 48usize;
            let mut flat = Vec::with_capacity(tensors.len() * 3 * h * width);
            for t in &tensors {
                flat.extend_from_slice(t);
            }
            let shape = [tensors.len() as i64, 3, h as i64, width as i64];
            let x = ort::value::Tensor::from_array((shape, flat))?;
            let outs = self.session.run(ort::inputs![x])?;
            let (_shape, preds) = outs[0].try_extract_tensor::<f32>()?;
            let classes = 6625usize;
            let steps = preds.len() / (tensors.len() * classes);
            for (r, &i) in batch.iter().enumerate() {
                let (text, conf, selection) = decode(
                    &preds[r * steps * classes..(r + 1) * steps * classes],
                    steps,
                    classes,
                    &self.characters,
                );
                results[i] = RecResult { text, score: conf, selection, steps };
            }
        }
        Ok(results)
    }
}

/// Greedy CTC decode: argmax per step, drop consecutive repeats, drop the
/// blank (index 0), map through the alphabet, score as the mean of the kept
/// steps' probabilities. An empty selection scores zero, like the Python.
fn decode(
    preds: &[f32],
    steps: usize,
    classes: usize,
    characters: &[String],
) -> (String, f32, Vec<usize>) {
    let mut last: usize = usize::MAX;
    let mut text = String::new();
    let mut confs: Vec<f32> = Vec::new();
    let mut selection: Vec<usize> = Vec::new();
    for t in 0..steps {
        let row = &preds[t * classes..(t + 1) * classes];
        let (bi, &bp) = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        if bi == 0 || bi == last {
            last = bi;
            continue;
        }
        last = bi;
        if let Some(c) = characters.get(bi) {
            text.push_str(c);
            confs.push(bp);
            selection.push(t);
        }
    }
    let conf = if confs.is_empty() {
        0.0
    } else {
        confs.iter().sum::<f32>() / confs.len() as f32
    };
    (text, conf, selection)
}
