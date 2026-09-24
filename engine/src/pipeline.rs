//! The whole pipeline: image in, text lines out, mirroring RapidOCR's
//! `RapidOCR.__call__` step for step - side-length normalisation, the wide
//! strip letterbox, detection, box sorting, perspective crops, the direction
//! classifier, recognition, the score filter, and the coordinate trip back
//! to the original image.

use anyhow::Result;
use image::{DynamicImage, RgbImage};

use crate::det::{self, DetParams};

pub struct OcrLine {
    pub box4: [(i32, i32); 4],
    pub text: String,
    pub score: f32,
}

pub struct Pipeline {
    pub det: ort::session::Session,
    pub cls: ort::session::Session,
    pub rec: crate::rec::RecModel,
    pub det_params: DetParams,
    /// Global config: wide strips wider than this ratio get letterboxed.
    pub width_height_ratio: f32,
    pub min_height: f32,
    pub max_side_len: usize,
    pub min_side_len: usize,
    pub text_score: f32,
}

struct Frame {
    img: RgbImage,
    /// ops in the order they were applied, so boxes can go back the way they
    /// came: ratios first, then the letterbox - the reverse pass subtracts and
    /// scales in the opposite order.
    ratio_h: f64,
    ratio_w: f64,
    pad_top: i64,
    pad_left: i64,
}

impl Pipeline {
    pub fn load(det_path: &std::path::Path, cls_path: &std::path::Path, rec_path: &std::path::Path) -> Result<Self> {
        let det = ort::session::Session::builder()?.commit_from_file(det_path)?;
        let cls = ort::session::Session::builder()?.commit_from_file(cls_path)?;
        let rec = crate::rec::RecModel::load(rec_path)?;
        Ok(Self {
            det,
            cls,
            rec,
            det_params: DetParams::default(),
            width_height_ratio: 8.0,
            min_height: 30.0,
            max_side_len: 2000,
            min_side_len: 30,
            text_score: 0.5,
        })
    }

    pub fn run(&mut self, raw: &DynamicImage) -> Result<Vec<OcrLine>> {
        let raw_w = raw.width() as usize;
        let raw_h = raw.height() as usize;
        let mut frame = self.normalise_sides(raw);
        self.maybe_letterbox(&mut frame);

        // Detection, in the transformed frame.
        let (input, nw, nh) = det::preprocess(&frame.img, &self.det_params);
        let x = ort::value::Tensor::from_array(([1i64, 3, nh as i64, nw as i64], input))?;
        let outs = self.det.run(ort::inputs![x])?;
        let (_shape, prob) = outs[0].try_extract_tensor::<f32>()?;
        let mut boxes = det::boxes_from_bitmap(prob, nw, nh, frame.img.width() as usize, frame.img.height() as usize, &self.det_params);
        sort_boxes(&mut boxes);

        // Perspective crops, upright, in recognition order.
        let mut crops: Vec<RgbImage> = boxes
            .iter()
            .map(|b| rotate_crop(&frame.img, &b.box4))
            .collect();

        // Direction classifier, then recognition.
        for c in crops.iter_mut() {
            let owned = std::mem::take(c);
            *c = crate::cls::maybe_rotate(&mut self.cls, &owned)?;
        }
        let rec = self.rec.recognize(&crops)?;
        if std::env::var("OCR_DEBUG").is_ok() {
            let good = rec.iter().filter(|(_, s)| *s >= self.text_score).count();
            eprintln!("  [debug] boxes={} crops={} rec>=0.5: {}", boxes.len(), crops.len(), good);
            for (t, sc) in rec.iter().take(8) {
                eprintln!("    [debug] {:.2} 「{}」", sc, t);
            }
        }

        // Score filter, then the coordinate trip back to the original image.
        let mut lines = Vec::new();
        for (b, (text, score)) in boxes.iter().zip(rec) {
            if score < self.text_score {
                continue;
            }
            lines.push(OcrLine { box4: b.box4, text, score });
        }
        for l in lines.iter_mut() {
            for c in l.box4.iter_mut() {
                let (mut x, mut y) = (c.0 as f64, c.1 as f64);
                x -= frame.pad_left as f64;
                y -= frame.pad_top as f64;
                x *= frame.ratio_w;
                y *= frame.ratio_h;
                *c = (
                    (x.round() as i64).clamp(0, raw_w as i64) as i32,
                    (y.round() as i64).clamp(0, raw_h as i64) as i32,
                );
            }
        }
        Ok(lines)
    }

    /// `preprocess`: shrink past the max side, grow past the min side. The
    /// ratios overwrite each other the way the Python reassigns them.
    fn normalise_sides(&self, raw: &DynamicImage) -> Frame {
        let (mut ratio_h, mut ratio_w) = (1.0f64, 1.0f64);
        let (mut w, mut h) = (raw.width() as f64, raw.height() as f64);
        if w.max(h) > self.max_side_len as f64 {
            let ratio = self.max_side_len as f64 / w.max(h);
            w = (w * ratio).floor();
            h = (h * ratio).floor();
            // The returned ratio is the trip BACK to the original size - the
            // origin mapping multiplies by it - so it is original over new.
            ratio_h = raw.height() as f64 / h;
            ratio_w = raw.width() as f64 / w;
        }
        if w.min(h) < self.min_side_len as f64 {
            let ratio = self.min_side_len as f64 / w.min(h);
            w = (w * ratio).floor();
            h = (h * ratio).floor();
            // Same convention, and the Python reassigns rather than
            // multiplies, so a double transform keeps only this ratio.
            ratio_h = raw.height() as f64 / h;
            ratio_w = raw.width() as f64 / w;
        }
        let rgb = raw.to_rgb8();
        let img = crate::det::bilinear_resize(&rgb, w as usize, h as usize);
        Frame { img, ratio_h, ratio_w, pad_top: 0, pad_left: 0 }
    }

    /// `maybe_add_letterbox`: very short or very wide strips get round-locked
    /// padding above and below so the detector has something to bite on.
    fn maybe_letterbox(&self, frame: &mut Frame) {
        let (w, h) = (frame.img.width() as f64, frame.img.height() as f64);
        let use_limit_ratio = w / h > self.width_height_ratio as f64;
        if h <= self.min_height as f64 || use_limit_ratio {
            let new_h = ((w / self.width_height_ratio as f64) as i64)
                .max(self.min_height as i64)
                * 2;
            let padding_h = ((new_h - h as i64).abs() / 2) as i64;
            if padding_h > 0 {
                let mut padded = RgbImage::new(frame.img.width(), (h as i64 + 2 * padding_h) as u32);
                image::imageops::overlay(&mut padded, &frame.img, padding_h as u32 as i64, 0);
                frame.img = padded;
                frame.pad_top = padding_h;
            }
        }
    }
}

/// `sorted_boxes`: top to bottom, then a bubble pass that keeps same-row
/// boxes (tops within ten pixels) left to right.
fn sort_boxes(boxes: &mut [det::TextBox]) {
    boxes.sort_by(|a, b| {
        let ka = (a.box4[0].1, a.box4[0].0);
        let kb = (b.box4[0].1, b.box4[0].0);
        ka.cmp(&kb)
    });
    let n = boxes.len();
    for i in 0..n.saturating_sub(1) {
        for j in (0..=i).rev() {
            let top_gap = (boxes[j + 1].box4[0].1 - boxes[j].box4[0].1).abs();
            if top_gap < 10 && boxes[j + 1].box4[0].0 < boxes[j].box4[0].0 {
                boxes.swap(j, j + 1);
            }
        }
    }
}

fn dist(a: (i32, i32), b: (i32, i32)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
}

/// `get_rotate_crop_image`: perspective-warp the quad into an upright rect,
/// replicating edges, then turn tall crops the right way round.
fn rotate_crop(img: &RgbImage, quad: &[(i32, i32); 4]) -> RgbImage {
    let p = [quad[0], quad[1], quad[2], quad[3]];
    let crop_w = dist(p[0], p[1]).max(dist(p[2], p[3])) as u32;
    let crop_h = dist(p[0], p[3]).max(dist(p[1], p[2])) as u32;
    if crop_w == 0 || crop_h == 0 {
        return RgbImage::new(1, 1);
    }
    let warped = perspective_warp(img, &p, crop_w, crop_h);
    if crop_h as f32 / crop_w as f32 >= 1.5 {
        image::imageops::rotate90(&warped)
    } else {
        warped
    }
}

/// Solve the 8-parameter homography for four point pairs, then inverse-map
/// every destination pixel with a cubic filter and clamped edges.
fn perspective_warp(img: &RgbImage, src: &[(i32, i32); 4], out_w: u32, out_h: u32) -> RgbImage {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    // The warp is inverse-mapped: every destination pixel asks "where does
    // this come from in the source", so the homography solved here is
    // dst -> src, not the forward one.
    let dst = [
        (0f64, 0f64),
        (out_w as f64, 0.0),
        (out_w as f64, out_h as f64),
        (0.0, out_h as f64),
    ];
    // Standard 8x8 system: x' = (h0 u + h1 v + h2)/(h6 u + h7 v + 1).
    let mut a = [[0f64; 8]; 8];
    let mut bvec = [0f64; 8];
    for i in 0..4 {
        let (x, y) = (src[i].0 as f64, src[i].1 as f64);
        let (u, v) = dst[i];
        a[i * 2] = [u, v, 1.0, 0.0, 0.0, 0.0, -x * u, -x * v];
        bvec[i * 2] = x;
        a[i * 2 + 1] = [0.0, 0.0, 0.0, u, v, 1.0, -y * u, -y * v];
        bvec[i * 2 + 1] = y;
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..8 {
        let mut piv = col;
        for r in col + 1..8 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        a.swap(col, piv);
        bvec.swap(col, piv);
        let d = a[col][col];
        if d.abs() < 1e-12 {
            continue;
        }
        for r in 0..8 {
            if r == col {
                continue;
            }
            let f = a[r][col] / d;
            for c in col..8 {
                a[r][c] -= f * a[col][c];
            }
            bvec[r] -= f * bvec[col];
        }
    }
    let h_: [f64; 8] = {
        let mut h = [0f64; 8];
        for i in 0..8 {
            h[i] = bvec[i] / a[i][i];
        }
        h
    };
    let map = |dx: f64, dy: f64| -> (f64, f64) {
        let den = h_[6] * dx + h_[7] * dy + 1.0;
        ((h_[0] * dx + h_[1] * dy + h_[2]) / den, (h_[3] * dx + h_[4] * dy + h_[5]) / den)
    };

    let cubic = |t: f64| -> f64 {
        // cv2's cubic convolution kernel, A = -0.75.
        let a = -0.75;
        let t = t.abs();
        if t < 1.0 {
            (a + 2.0) * t * t * t - (a + 3.0) * t * t + 1.0
        } else if t < 2.0 {
            a * t * t * t - 5.0 * a * t * t + 8.0 * a * t - 4.0 * a
        } else {
            0.0
        }
    };
    let sample = |sx: f64, sy: f64| -> [f64; 3] {
        let sx = sx.clamp(0.0, (iw - 1) as f64);
        let sy = sy.clamp(0.0, (ih - 1) as f64);
        let x0 = sx.floor() as i64;
        let y0 = sy.floor() as i64;
        let fx = sx - x0 as f64;
        let fy = sy - y0 as f64;
        let mut acc = [0f64; 3];
        for m in -1..=2 {
            for n in -1..=2 {
                let xx = (x0 + n).clamp(0, iw - 1) as u32;
                let yy = (y0 + m).clamp(0, ih - 1) as u32;
                let px = img.get_pixel(xx, yy);
                let wgt = cubic(n as f64 - fx) * cubic(m as f64 - fy);
                for ch in 0..3 {
                    acc[ch] += px[ch] as f64 * wgt;
                }
            }
        }
        acc
    };

    let mut out = RgbImage::new(out_w, out_h);
    for dy in 0..out_h {
        for dx in 0..out_w {
            let (sx, sy) = map(dx as f64, dy as f64);
            let v = sample(sx, sy);
            out.put_pixel(
                dx,
                dy,
                image::Rgb([v[0].clamp(0.0, 255.0) as u8, v[1].clamp(0.0, 255.0) as u8, v[2].clamp(0.0, 255.0) as u8]),
            );
        }
    }
    out
}
