//! 识别结果 → .docx。
//!
//! ── 为什么不「一行一段」 ──
//!
//! 扫描件的每一视觉行如果各成一段，用户在 Word 里改一个字就要先合并段落：
//! 「这行没写完」被硬编码成了段落边界。识别阶段本来就有每行的四点框，
//! 几何足够把行并回段落，所以这里按三条判据决定新段落：
//!
//!   ① 行距：本行顶 − 上一行底 > 0.8 倍行高 —— 段间空档；
//!   ② 缩进：本行左边界比当前段左边界右移 > 1.2 倍行高 —— 首行缩进；
//!   ③ 提前换行：上一行右边界离整页右沿还差 > 2 倍行高，且本行回到段左边界
//!      —— 上一行是这段的最后一行。
//!
//! 判据只用**相对**量（行高的倍数），不假设 DPI：识别图可以是 200 dpi 也可以是
//! 600 dpi，同一页内的行高倍数不变。
//!
//! ── 已经带文字层的页面 ──
//!
//! 这类页面没有框，只有 PDF 自己的文字。按空行分段，段内连续行合并 —— 与
//! 上面的结果同形，避免同一份 Word 里一半段落合并过、一半是硬换行。
//!
//! ── 不做的事 ──
//!
//! 不猜标题级别、不还原表格、不还原分栏、不套学术模板。识别出来的是**文字**，
//! 排版交回给使用者在 Word 里做。给一个诚实的纯文字稿，比给一个猜错的版式有用。

use docx_rs::{
    Docx, LineSpacing, LineSpacingType, PageMargin, PageOrientationType, Paragraph, Run, RunFonts,
};

/// 导出结果，回给前端做一句状态栏文案。
pub struct Stats {
    pub pages: usize,
    pub paragraphs: usize,
    pub lines: usize,
}

/// 一行字的最小几何。坐标沿用识别图（像素），只在页内比较。
#[derive(Clone, Debug)]
pub struct LineBox {
    pub text: String,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl LineBox {
    pub fn from_quad(text: &str, quad: &[(i32, i32); 4]) -> Self {
        let xs = quad.iter().map(|p| p.0 as f32);
        let ys = quad.iter().map(|p| p.1 as f32);
        let (mut left, mut right) = (f32::MAX, f32::MIN);
        let (mut top, mut bottom) = (f32::MAX, f32::MIN);
        for x in xs {
            left = left.min(x);
            right = right.max(x);
        }
        for y in ys {
            top = top.min(y);
            bottom = bottom.max(y);
        }
        Self { text: text.to_string(), top, bottom, left, right }
    }

    fn height(&self) -> f32 {
        (self.bottom - self.top).max(1.0)
    }
}

/// 中位数。判据都建立在「这一页的行高是多少」上，用中位数而不是均值：
/// 一页里混进一条竖排标尺或一个大标题，均值和方差都会歪。
fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// 两侧都是中日韩字符时不插空格，否则插一个 —— 与家族其余产品的拼接规则一致。
fn cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF     // 日文假名
        | 0x3400..=0x4DBF   // 扩展 A
        | 0x4E00..=0x9FFF   // 基本区
        | 0xF900..=0xFAFF   // 兼容区
        | 0xAC00..=0xD7AF   // 谚文
        | 0x3000..=0x303F)  // 中日韩标点
}

fn join(acc: &mut String, next: &str) {
    if acc.is_empty() {
        acc.push_str(next);
        return;
    }
    let last = acc.chars().last().unwrap_or(' ');
    let first = next.chars().next().unwrap_or(' ');
    if !(cjk(last) && cjk(first)) {
        acc.push(' ');
    }
    acc.push_str(next);
}

/// 一页的行 → 段落。返回每段的文字。
pub fn paragraphs_from_lines(lines: &[LineBox]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if lines.is_empty() {
        return out;
    }
    let h = median(lines.iter().map(|l| l.height()).collect());
    // 段左边界取全页最常见（这里用中位数近似）的左边界：缩进判据要有基准。
    let para_left = median(lines.iter().map(|l| l.left).collect());
    let page_right = lines.iter().map(|l| l.right).fold(f32::MIN, f32::max);

    let mut acc = String::new();
    let mut prev: Option<&LineBox> = None;

    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(p) = prev {
            let gap = line.top - p.bottom;
            let indented = line.left - para_left > 1.2 * h;
            let prev_short = page_right - p.right > 2.0 * h;
            let back_to_margin = line.left - para_left <= 0.6 * h;
            if gap > 0.8 * h || indented || (prev_short && back_to_margin) {
                out.push(std::mem::take(&mut acc));
            }
        }
        join(&mut acc, text);
        prev = Some(line);
    }
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

/// 已经带文字层的一页 → 段落：空行分段，段内合并。
pub fn paragraphs_from_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if !acc.is_empty() {
                out.push(std::mem::take(&mut acc));
            }
            continue;
        }
        join(&mut acc, line);
    }
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

fn body_fonts() -> RunFonts {
    // 西文 Times New Roman、中文宋体：一份中文文档在 Word 里的常态，
    // 也是「扫描件数字化」最不喧宾夺主的组合。
    RunFonts::new().ascii("Times New Roman").east_asia("宋体")
}

fn body(text: &str) -> Paragraph {
    Paragraph::new()
        .add_run(Run::new().size(24).fonts(body_fonts()).add_text(text))
        .line_spacing(
            LineSpacing::new()
                .line(240)
                .line_rule(LineSpacingType::Auto),
        )
}

/// 写出一份 .docx。
///
/// `pages` 每页若干段文字。页与页之间插分页符，与原件页码一一对应 ——
/// 少了它，导出稿的页数就跟原件对不上，对稿子的人会错行。
pub fn write(
    pages: &[Vec<String>],
    page_twips: Option<(i32, i32)>,
    output: &str,
) -> anyhow::Result<Stats> {
    let (w, h) = page_twips.unwrap_or((11906, 16838)); // 缺省 A4 纵向
    let mut doc = Docx::new()
        .page_size(w, h)
        .page_margin(PageMargin {
            top: 1440,
            left: 1440,
            bottom: 1440,
            right: 1440,
            header: 720,
            footer: 720,
            gutter: 0,
        })
        .page_orient(PageOrientationType::Portrait);

    let mut paragraphs = 0usize;
    let mut lines = 0usize;
    for (i, page) in pages.iter().enumerate() {
        for (j, text) in page.iter().enumerate() {
            let mut p = body(text);
            // 每页第一段带分页符（首页不带）。
            if i > 0 && j == 0 {
                p = p.page_break_before(true);
            }
            doc = doc.add_paragraph(p);
            paragraphs += 1;
        }
        lines += page.len();
    }

    // 空文档也要有正文：Word 打不开零段落的 docx。
    if paragraphs == 0 {
        doc = doc.add_paragraph(body(""));
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut buf)?;
    let bytes = buf.into_inner();

    let path = std::path::Path::new(output);
    let tmp = path.with_file_name(format!(
        ".{}.{}.docx.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("ocr"),
        std::process::id()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;

    Ok(Stats { pages: pages.len(), paragraphs, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lb(text: &str, top: f32, left: f32, right: f32) -> LineBox {
        // 行高统一 40，便于把判据写成「行高的倍数」
        LineBox { text: text.into(), top, bottom: top + 40.0, left, right }
    }

    #[test]
    fn wrapped_lines_merge_into_one_paragraph() {
        // 三行连续、左边界一致、行距正常 → 一段
        let lines = vec![
            lb("The shop does not open until nine, and I", 100.0, 50.0, 950.0),
            lb("have stopped pretending I sleep past six.", 145.0, 50.0, 950.0),
            lb("Coffee, the window, the street.", 190.0, 50.0, 940.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("nine, and I have stopped"), "{}", p[0]);
    }

    #[test]
    fn blank_gap_starts_a_new_paragraph() {
        let lines = vec![
            lb("First paragraph ends here.", 100.0, 50.0, 900.0),
            lb("Second paragraph starts here.", 220.0, 50.0, 780.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[0], "First paragraph ends here.");
    }

    #[test]
    fn short_line_followed_by_margin_line_starts_a_new_paragraph() {
        // 行距只有 45（正常行距），但上一行在 400 处就结束了 → 段末
        let lines = vec![
            lb("A paragraph that ends early", 100.0, 50.0, 400.0),
            lb("and the next one starts at the margin.", 145.0, 50.0, 900.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 2, "{p:?}");
    }

    #[test]
    fn first_line_indent_starts_a_new_paragraph() {
        let lines = vec![
            lb("Body line that runs to the right margin here.", 100.0, 50.0, 950.0),
            lb("    Indented opening of the next paragraph.", 145.0, 130.0, 950.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[1], "Indented opening of the next paragraph.");
    }

    #[test]
    fn cjk_lines_join_without_spaces() {
        let lines = vec![
            lb("这家店九点才开门，我早就不装作", 100.0, 50.0, 950.0),
            lb("自己能睡到六点以后了。", 145.0, 50.0, 950.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0], "这家店九点才开门，我早就不装作自己能睡到六点以后了。");
    }

    #[test]
    fn latin_join_inserts_a_space() {
        let mut acc = String::from("end of");
        join(&mut acc, "the line");
        assert_eq!(acc, "end of the line");
    }

    #[test]
    fn native_text_splits_on_blank_lines() {
        let text = "line one\nline two\n\nline three\n";
        let p = paragraphs_from_text(text);
        assert_eq!(p, vec!["line one line two", "line three"]);
    }

    #[test]
    fn empty_page_yields_no_paragraphs() {
        assert!(paragraphs_from_lines(&[]).is_empty());
        assert!(paragraphs_from_text("   \n\n").is_empty());
    }

    #[test]
    fn write_produces_a_readable_docx() {
        let dir = std::env::temp_dir().join(format!("ocr-docx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.docx");
        let pages = vec![
            vec!["page one paragraph".to_string()],
            vec!["page two paragraph".to_string()],
        ];
        let s = write(&pages, None, out.to_str().unwrap()).unwrap();
        assert_eq!(s.pages, 2);
        assert_eq!(s.paragraphs, 2);

        // 落盘的必须是可解析的 docx：zip 头 PK，且能被 docx-rs 读回
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"PK"), "不是 zip 容器");
        assert!(docx_rs::read_docx(&bytes).is_ok(), "docx 读回失败");
        std::fs::remove_dir_all(&dir).ok();
    }
}
