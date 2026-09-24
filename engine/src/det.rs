//! The detector: preprocessing exactly the way RapidOCR does it, and the DB
//! post-process ported from its `DBPostProcess`, contour-for-contour.
//!
//! cv2 capabilities are reimplemented rather than depended on: `findContours`
//! becomes boundary tracing, `minAreaRect` becomes a convex hull plus rotating
//! calipers, `fillPoly` becomes an even-odd rasterisation, and pyclipper's
//! round-joint offset becomes the `clipper2` crate. Small pixel-level
//! differences are possible; the double-engine harness decides whether they
//! matter.

use anyhow::Result;

pub struct DetParams {
    /// Scale the smaller side to this only when it is at least this already.
    pub limit_side_len: usize,
    pub thresh: f32,
    pub box_thresh: f32,
    pub max_candidates: usize,
    pub unclip_ratio: f32,
    /// RapidOCR config sets this true: a 2x2 max filter before contouring.
    pub use_dilation: bool,
    pub min_size: f32,
}

impl Default for DetParams {
    fn default() -> Self {
        Self {
            limit_side_len: 736,
            thresh: 0.3,
            box_thresh: 0.5,
            max_candidates: 1000,
            unclip_ratio: 1.6,
            use_dilation: true,
            min_size: 3.0,
        }
    }
}

/// Preprocess an RGB image the way `DetPreProcess` does.
///
/// Returns the NCHW tensor plus the network input size, so the post-process
/// can scale boxes back to the original width and height it is given.
pub fn preprocess(img: &image::RgbImage, p: &DetParams) -> (Vec<f32>, usize, usize) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    // limit_type "min": a smaller side below the limit is scaled UP to meet
    // it; a larger image is left at ratio 1.0.
    let scale = if w.min(h) < p.limit_side_len {
        p.limit_side_len as f32 / w.min(h) as f32
    } else {
        1.0
    };
    // cv2.resize to the rounded size happens even when the ratio is 1.0: the
    // image always lands on a multiple of thirty-two.
    // The resize lands directly on the thirty-two multiple: cv2.resize
    // stretches the image to the rounded size, there is no padding step.
    let rw = (w as f32 * scale) as usize;
    let rh = (h as f32 * scale) as usize;
    let nw = (rw as f32 / 32.0).round() as usize * 32;
    let nh = (rh as f32 / 32.0).round() as usize * 32;

    let resized = if (nw, nh) == (w, h) {
        img.clone()
    } else {
        image::imageops::resize(
            img,
            nw as u32,
            nh as u32,
            image::imageops::FilterType::Triangle,
        )
    };

    let mut input = vec![0f32; nw * nh * 3];
    for y in 0..nh {
        for x in 0..nw {
            let px = resized.get_pixel(x as u32, y as u32);
            let base = y * nw + x;
            input[base] = (px[0] as f32 / 255.0 - 0.5) / 0.5;
            input[base + nw * nh] = (px[1] as f32 / 255.0 - 0.5) / 0.5;
            input[base + 2 * nw * nh] = (px[2] as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    (input, nw, nh)
}

/// Binary map from the probability map, dilated the way the config asks.
fn dilate2x2(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    // cv2.dilate with a 2x2 kernel anchored at (0, 0): each output pixel is
    // the max over the pixel and its right, bottom and bottom-right friends.
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let v = mask[y * w + x]
                || (x + 1 < w && mask[y * w + x + 1])
                || (y + 1 < h && mask[(y + 1) * w + x])
                || (y + 1 < h && x + 1 < w && mask[(y + 1) * w + x + 1]);
            out[y * w + x] = v;
        }
    }
    out
}

/// Connected components of the foreground, as pixel lists.
///
/// `cv2.findContours` would produce ordered boundary polygons plus hole
/// boundaries, but nothing downstream wants the order: `minAreaRect` takes
/// the convex hull, which a component's full pixel set determines as well as
/// its boundary does, and the score is computed from the quad. Hole contours
/// (the inner ring of a large "O", say) are dropped; the harness will say
/// whether any real box was lost to that.
pub fn connected_components(mask: &[bool], w: usize, h: usize) -> Vec<Vec<(i64, i64)>> {
    let mut visited = vec![false; w * h];
    let mut components = Vec::new();
    let mut stack: Vec<(i64, i64)> = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            if !mask[sy * w + sx] || visited[sy * w + sx] {
                continue;
            }
            stack.push((sx as i64, sy as i64));
            visited[sy * w + sx] = true;
            let mut pixels = Vec::new();
            while let Some((cx, cy)) = stack.pop() {
                pixels.push((cx, cy));
                for (dx, dy) in [
                    (1i64, 0i64),
                    (0, 1),
                    (-1, 0),
                    (0, -1),
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                ] {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    if mask[ni] && !visited[ni] {
                        visited[ni] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            components.push(pixels);
        }
    }
    components
}

fn convex_hull(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut lower = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Minimum-area rotated rectangle of a point set: convex hull plus the
/// calipers. Returns the four corners in cv2's boxPoints order and the
/// smaller side, which is what `get_mini_boxes` reports as `sside`.
fn min_area_rect(points: &[(i64, i64)]) -> ([[(f64, f64); 4]; 1], f32) {
    let hull = convex_hull(points.iter().map(|&(x, y)| (x as f64, y as f64)).collect());
    if hull.len() < 3 {
        return ([[hull
            .first()
            .copied()
            .unwrap_or((0.0, 0.0)); 4]], 0.0);
    }
    let mut best_area = f64::MAX;
    let mut best: [[(f64, f64); 4]; 1] = [[(0.0, 0.0); 4]];
    let mut best_min_side = f32::MAX;
    let n = hull.len();
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (vx, vy) = (-uy, ux);
        let (mut min_u, mut max_u, mut min_v, mut max_v) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &hull {
            let pu = p.0 * ux + p.1 * uy;
            let pv = p.0 * vx + p.1 * vy;
            min_u = min_u.min(pu);
            max_u = max_u.max(pu);
            min_v = min_v.min(pv);
            max_v = max_v.max(pv);
        }
        let area = (max_u - min_u) * (max_v - min_v);
        if area < best_area {
            best_area = area;
            let (mw, mh) = (max_u - min_u, max_v - min_v);
            // A point with projections (pu, pv) sits at pu*u + pv*v in xy
            // space, so the corners are the extreme projections mapped back.
            let c0 = (min_u * ux + min_v * vx, min_u * uy + min_v * vy);
            let c1 = (max_u * ux + min_v * vx, max_u * uy + min_v * vy);
            let c2 = (max_u * ux + max_v * vx, max_u * uy + max_v * vy);
            let c3 = (min_u * ux + max_v * vx, min_u * uy + max_v * vy);
            best = [[c0, c1, c2, c3]];
            best_min_side = mw.min(mh) as f32;
        }
    }
    (best, best_min_side)
}

/// `get_mini_boxes`: the minimum-area rect, corners sorted by x and reordered
/// by y exactly as the Python does, plus the smaller rect side.
fn mini_boxes(contour: &[(i64, i64)]) -> ([(f64, f64); 4], f32) {
    let (rect, sside) = min_area_rect(contour);
    let mut points = rect[0];
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (i1, i2, i3, i4);
    if points[1].1 > points[0].1 {
        i1 = 0;
        i4 = 1;
    } else {
        i1 = 1;
        i4 = 0;
    }
    if points[3].1 > points[2].1 {
        i2 = 2;
        i3 = 3;
    } else {
        i2 = 3;
        i3 = 2;
    }
    ([points[i1], points[i2], points[i3], points[i4]], sside)
}

/// `box_score_fast`: mean of the probability map inside the polygon.
fn box_score_fast(prob: &[f32], w: usize, h: usize, box4: [(f64, f64); 4]) -> f32 {
    let xmin = (box4.iter().map(|p| p.0).fold(f64::MAX, f64::min).floor() as usize).min(w - 1);
    let xmax = (box4.iter().map(|p| p.0).fold(f64::MIN, f64::max).ceil() as usize).min(w - 1);
    let ymin = (box4.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor() as usize).min(h - 1);
    let ymax = (box4.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil() as usize).min(h - 1);
    if xmin > xmax || ymin > ymax {
        return 0.0;
    }
    // Even-odd point-in-polygon at integer pixel positions, the way fillPoly
    // rasterises its integer vertices.
    let inside = |px: f64, py: f64| -> bool {
        let mut inside = false;
        let mut j = 3;
        for i in 0..4 {
            let (xi, yi) = box4[i];
            let (xj, yj) = box4[j];
            if (yi > py) != (yj > py)
                && px < (xj - xi) * (py - yi) / (yj - yi + f64::MIN_POSITIVE) + xi
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    };
    let mut sum = 0.0f64;
    let mut count = 0u64;
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            if inside(x as f64, y as f64) {
                sum += prob[y * w + x] as f64;
            }
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

/// `unclip`: offset the quad outward by `area * ratio / perimeter`, round
/// joins, and flatten every returned path the way the Python reshape does.
fn unclip(box4: [(f64, f64); 4], ratio: f32) -> Vec<(f64, f64)> {
    let poly_area = |p: &[(f64, f64); 4]| -> f64 {
        let mut a = 0.0;
        for i in 0..4 {
            let j = (i + 1) % 4;
            a += p[i].0 * p[j].1 - p[j].0 * p[i].1;
        }
        (a / 2.0).abs()
    };
    let perimeter = {
        let mut l = 0.0;
        for i in 0..4 {
            let j = (i + 1) % 4;
            l += ((box4[j].0 - box4[i].0).powi(2) + (box4[j].1 - box4[i].1).powi(2)).sqrt();
        }
        l
    };
    if perimeter < 1e-6 {
        return box4.to_vec();
    }
    let distance = poly_area(&box4) * ratio as f64 / perimeter;

    // Offsetting goes outward only for positively oriented paths, so the quad
    // is normalised to counter-clockwise first (shoelace sign tells which).
    let signed = (box4[1].0 - box4[0].0) * (box4[2].1 - box4[0].1)
        - (box4[2].0 - box4[0].0) * (box4[1].1 - box4[0].1);
    let oriented: Vec<(f64, f64)> = if signed < 0.0 {
        box4.iter().rev().copied().collect()
    } else {
        box4.to_vec()
    };

    let paths: clipper2::Paths = vec![clipper2::Path::new(
        oriented
            .iter()
            .map(|&(x, y)| clipper2::Point::new(x, y))
            .collect::<Vec<_>>(),
    )]
    .into();
    let out = clipper2::inflate(
        paths,
        distance,
        clipper2::JoinType::Round,
        clipper2::EndType::Polygon,
        2.0,
    );
    out.iter().flat_map(|p| p.iter().map(|pt| (pt.x(), pt.y()))).collect()
}

pub struct TextBox {
    /// Four corners, ordered the way `get_mini_boxes` orders them, in
    /// original-image pixel coordinates.
    pub box4: [(i32, i32); 4],
    pub score: f32,
}

/// The DB post-process: probability map to text boxes in original-image
/// coordinates. `src_w`/`src_h` are the ORIGINAL image size the boxes scale
/// back to, exactly as the Python passes `src_w, src_h`.
pub fn boxes_from_bitmap(
    prob: &[f32],
    w: usize,
    h: usize,
    src_w: usize,
    src_h: usize,
    p: &DetParams,
) -> Vec<TextBox> {
    let mut mask: Vec<bool> = prob.iter().map(|&v| v > p.thresh).collect();
    if p.use_dilation {
        mask = dilate2x2(&mask, w, h);
    }
    let contours = connected_components(&mask, w, h);

    let mut boxes = Vec::new();
    for contour in contours.iter().take(p.max_candidates) {
        let (points, sside) = mini_boxes(contour);
        if sside < p.min_size {
            continue;
        }
        let score = box_score_fast(prob, w, h, points);
        if p.box_thresh > score {
            continue;
        }
        let expanded = unclip(points, p.unclip_ratio);
        if expanded.len() < 4 {
            continue;
        }
        let (box4, sside2) = mini_boxes(&expanded.iter().map(|&(x, y)| (x as i64, y as i64)).collect::<Vec<_>>());
        if sside2 < p.min_size + 2.0 {
            continue;
        }
        let conv: [(i32, i32); 4] = box4.map(|c| {
            let x = ((c.0 / w as f64 * src_w as f64).round() as i32).clamp(0, src_w as i32);
            let y = ((c.1 / h as f64 * src_h as f64).round() as i32).clamp(0, src_h as i32);
            (x, y)
        });
        boxes.push(TextBox { box4: conv, score });
    }
    boxes
}

/// One-shot: preprocess, run the detector, post-process. Kept here so the
/// example and the future binary share the exact same path.
pub fn detect(
    session: &mut ort::session::Session,
    img: &image::RgbImage,
    p: &DetParams,
) -> Result<Vec<TextBox>> {
    let (input, nw, nh) = preprocess(img, p);
    let shape = [1i64, 3, nh as i64, nw as i64];
    let x = ort::value::Tensor::from_array((shape, input))?;
    let outs = session.run(ort::inputs![x])?;
    let (_shape, data) = outs[0].try_extract_tensor::<f32>()?;
    // The network sees the padded size; the map is (nh, nw).
    Ok(boxes_from_bitmap(data, nw, nh, img.width() as usize, img.height() as usize, p))
}
