//! Per-word boxes: the port of `cal_rec_boxes`.
//!
//! The recogniser yields, per line, the decoded text plus which network steps
//! produced which character. This module turns that into a quad per word
//! (per character for CJK) in the original image: cells across the crop's
//! width, CJK and latin sized by their own average cell width, overlap
//! adjusted, then mapped back through the crop's homography.

/// One decoded step's contribution, as `get_word_info` groups them.
pub struct WordInfo {
    /// Total network steps for this line (the cell-count denominator).
    pub col_num: usize,
    /// Groups of characters: CJK runs per character, latin runs per word.
    pub word_list: Vec<Vec<char>>,
    /// Network column index of each character in each group.
    pub word_col_list: Vec<Vec<usize>>,
    /// "cn" or "en&num" per group.
    pub state_list: Vec<String>,
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

/// `get_word_info`: group decoded characters into words, recording each
/// character's column. A group breaks when the script class changes or when
/// the gap between consecutive decoded columns exceeds four steps.
pub fn get_word_info(text: &[char], selection: &[usize]) -> (Vec<Vec<char>>, Vec<Vec<usize>>, Vec<String>) {
    let mut word_list = Vec::new();
    let mut word_col_list = Vec::new();
    let mut state_list = Vec::new();
    if selection.is_empty() {
        return (word_list, word_col_list, state_list);
    }
    // col_width[0] is the run-up before the first decoded column, capped by
    // the kind of character that starts the line.
    let mut col_width = vec![0f64; selection.len()];
    for i in 1..selection.len() {
        col_width[i] = (selection[i] - selection[i - 1]) as f64;
    }
    let first = text.first().copied().unwrap_or('a');
    col_width[0] = 3.0f64.min(if is_cjk(first) { 3.0 } else { 2.0 }).min(selection[0] as f64);

    let mut state: Option<String> = None;
    let mut word_content: Vec<char> = Vec::new();
    let mut word_col_content: Vec<usize> = Vec::new();
    for (ci, &ch) in text.iter().enumerate() {
        let c_state = if is_cjk(ch) { "cn" } else { "en&num" }.to_string();
        if state.is_none() {
            state = Some(c_state.clone());
        }
        let cur = state.clone().unwrap();
        if cur != c_state || col_width[ci] > 4.0 {
            if !word_content.is_empty() {
                word_list.push(word_content.clone());
                word_col_list.push(word_col_content.clone());
                state_list.push(cur.clone());
                word_content.clear();
                word_col_content.clear();
            }
            state = Some(c_state.clone());
        }
        word_content.push(ch);
        word_col_content.push(selection[ci]);
    }
    if !word_content.is_empty() {
        word_list.push(word_content);
        word_col_list.push(word_col_content);
        state_list.push(state.unwrap());
    }
    (word_list, word_col_list, state_list)
}

/// A word's box in the crop's coordinate frame, before the trip back.
struct CropWord {
    text: String,
    x0: f64,
    x1: f64,
}

/// `cal_ocr_word_box` + `adjust_box_overlap`, on a crop of width `w` and
/// height `h`.
fn cal_word_boxes(
    text: &[char],
    w: usize,
    h: usize,
    info: &WordInfo,
) -> Vec<CropWord> {
    let bbox_x_start = 0.0f64;
    let bbox_x_end = w as f64;
    let bbox_y_start = 0.0f64;
    let bbox_y_end = h as f64;
    let cell_width = (bbox_x_end - bbox_x_start) / info.col_num as f64;

    let mut cn_widths: Vec<f64> = Vec::new();
    let mut en_widths: Vec<f64> = Vec::new();
    let mut cn_cols: Vec<usize> = Vec::new();
    let mut en_cols: Vec<usize> = Vec::new();
    let mut cn_content: Vec<char> = Vec::new();
    let mut en_content: Vec<char> = Vec::new();

    // cal_char_width: the average cell width of one multi-column word.
    let char_width = |cols: &[usize]| -> Option<f64> {
        if cols.len() < 2 {
            return None;
        }
        Some((cols[cols.len() - 1] - cols[0]) as f64 * cell_width / (cols.len() - 1) as f64)
    };

    for ((word, cols), state) in info.word_list.iter().zip(&info.word_col_list).zip(&info.state_list) {
        if let Some(cw) = char_width(cols) {
            if state == "cn" {
                cn_widths.push(cw);
            } else {
                en_widths.push(cw);
            }
        }
        if state == "cn" {
            cn_cols.extend(cols.iter().copied());
            cn_content.extend(word.iter().copied());
        } else {
            en_cols.extend(cols.iter().copied());
            en_content.extend(word.iter().copied());
        }
    }

    let mut out: Vec<CropWord> = Vec::new();
    let mut cal_box = |cols: &[usize], widths: &[f64], content: &[char]| {
        if cols.is_empty() {
            return;
        }
        let avg_char_width = if !widths.is_empty() {
            widths.iter().sum::<f64>() / widths.len() as f64
        } else {
            (bbox_x_end - bbox_x_start) / text.len().max(1) as f64
        };
        for &center_idx in cols {
            let center_x = (center_idx as f64 + 0.5) * cell_width;
            let cell_x_start =
                (center_x - avg_char_width / 2.0).max(0.0).floor() as i64 as f64 + bbox_x_start;
            let cell_x_end = (center_x + avg_char_width / 2.0)
                .min(bbox_x_end - bbox_x_start)
                .floor() as i64 as f64
                + bbox_x_start;
            out.push(CropWord {
                text: String::new(),
                x0: cell_x_start,
                x1: cell_x_end,
            });
        }
        // Content follows the same order as the boxes.
        let mut ci = out.len() - cols.len();
        for ch in content {
            if let Some(cw) = out.get_mut(ci) {
                cw.text.push(*ch);
            }
            ci += 1;
        }
    };
    cal_box(&cn_cols, &cn_widths, &cn_content);
    cal_box(&en_cols, &en_widths, &en_content);

    // Word boxes come out CJK-first and latin-second; re-sort by x.
    out.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());

    // adjust_box_overlap: split any overlap evenly between neighbours.
    for i in 0..out.len().saturating_sub(1) {
        let (before, after) = out.split_at_mut(i + 1);
        let cur = &mut before[i];
        let nxt = &mut after[0];
        if cur.x1 > nxt.x0 {
            let distance = cur.x1 - nxt.x0;
            cur.x1 -= distance / 2.0;
            nxt.x0 += distance - distance / 2.0;
        }
    }
    out
}

/// Order four points top-left, top-right, bottom-right, bottom-left. The
/// sum/diff trick orders rectangles; anything else falls back to an angular
/// sort around the centroid.
fn order_points(pts: Vec<(f64, f64)>) -> [(f64, f64); 4] {
    let cx = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
    let cy = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
    let mut sorted = pts;
    sorted.sort_by(|a, b| {
        let aa = (a.0 - cx).atan2(a.1 - cy);
        let bb = (b.0 - cx).atan2(b.1 - cy);
        aa.partial_cmp(&bb).unwrap()
    });
    // Angular sort runs clockwise-from-south in image coordinates; rotate so
    // the smallest-sum point (top-left) leads.
    let lead = sorted
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let sa = a.1 .0 + a.1 .1;
            let sb = b.1 .0 + b.1 .1;
            sa.partial_cmp(&sb).unwrap()
        })
        .unwrap()
        .0;
    let mut out = [(0.0, 0.0); 4];
    for k in 0..4 {
        out[k] = sorted[(lead + k) % 4];
    }
    out
}

/// Map a point from the crop's frame back to the original image through the
/// inverse of the crop's homography. `quad` is the detection box.
fn crop_to_image(pt: (f64, f64), quad: &[(f64, f64); 4], crop_w: f64, crop_h: f64, direction: &str) -> (f64, f64) {
    let (mut px, mut py) = pt;
    if direction == "h" {
        // Undo np.rot90's counter-clockwise turn: rotate clockwise about the
        // origin, then shift onto the crop's width.
        let (nx, ny) = (-py, px);
        px = nx + crop_w;
        py = ny;
    }
    // The crop's own frame, normalised to its top-left corner.
    let left = quad
        .iter()
        .map(|p| p.0)
        .fold(f64::MAX, f64::min);
    let top = quad
        .iter()
        .map(|p| p.1)
        .fold(f64::MAX, f64::min);
    let s: [(f64, f64); 4] = [
        (quad[0].0 - left, quad[0].1 - top),
        (quad[1].0 - left, quad[1].1 - top),
        (quad[2].0 - left, quad[2].1 - top),
        (quad[3].0 - left, quad[3].1 - top),
    ];
    let d = [(0.0, 0.0), (crop_w, 0.0), (crop_w, crop_h), (0.0, crop_h)];
    let inv = crate::pipeline::homography(&d, &s);
    let den = inv[6] * px + inv[7] * py + 1.0;
    let x = (inv[0] * px + inv[1] * py + inv[2]) / den + left;
    let y = (inv[3] * px + inv[4] * py + inv[5]) / den + top;
    (x, y)
}

/// The full pass: text, word info and crop geometry in, word quads in the
/// original image out.
pub fn word_boxes(
    text: &str,
    selection: &[usize],
    steps: usize,
    crop: (usize, usize),
    quad: &[(f64, f64); 4],
) -> Vec<(String, [(i64, i64); 4])> {
    let chars: Vec<char> = text.chars().collect();
    // get_word_info runs on the decoded text and selection; the cell-count
    // denominator is the full step count, blanks included.
    let (word_list, word_col_list, state_list) = get_word_info(&chars, selection);
    let info = WordInfo {
        col_num: steps.max(1),
        word_list,
        word_col_list,
        state_list,
    };
    let (crop_w, crop_h) = (crop.0 as f64, crop.1 as f64);
    let direction = if crop_h / crop_w >= 1.5 { "h" } else { "w" };
    let words = cal_word_boxes(&chars, crop.0, crop.1, &info);
    let mut out = Vec::new();
    for cw in words {
        if cw.text.is_empty() || cw.x1 <= cw.x0 {
            continue;
        }
        let corners = [
            (cw.x0, 0.0),
            (cw.x1, 0.0),
            (cw.x1, crop_h),
            (cw.x0, crop_h),
        ];
        let mapped: Vec<(f64, f64)> = corners
            .iter()
            .map(|&(x, y)| crop_to_image((x, y), quad, crop_w, crop_h, direction))
            .collect();
        let ordered = order_points(mapped);
        let conv: [(i64, i64); 4] = ordered.map(|(x, y)| (x.round() as i64, y.round() as i64));
        out.push((cw.text.clone(), conv));
    }
    out
}
