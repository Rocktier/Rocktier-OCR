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
//!
//! 唯一的例外是首行缩进：它不是猜出来的样式，而是原件里量得到的几何 ——
//! 既然切段都用到了它，写盘时就还原成「首行缩进 2 字符」。否则一份靠缩进分段
//! 的中文稿导出后通篇顶格，读者还得自己一段段去设。

use docx_rs::{
    Docx, LineSpacing, LineSpacingType, PageMargin, PageOrientationType, Paragraph, Run, RunFonts,
    SpecialIndentType,
};

/// 导出结果，回给前端做一句状态栏文案。
pub struct Stats {
    pub pages: usize,
    pub paragraphs: usize,
}

/// 一段：文字，和它在原件里是不是首行缩进的。
///
/// 缩进不只是「怎么切段」的判据，它本身就是排版信息：中文稿几乎全靠每段首行
/// 缩两字来分段。把行并成一段却把缩进丢掉，导出稿在 Word 里就是通篇顶格 ——
/// 读的人还得自己一段段去设缩进。这里把这一位留下，写盘时还原成
/// 「首行缩进 2 字符」。
#[derive(Clone, Debug, PartialEq)]
pub struct Para {
    pub text: String,
    pub first_line_indent: bool,
}

impl Para {
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), first_line_indent: false }
    }
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

/// 低分位数。左边距 = 少数行才会缩进，所以取偏小的那一档：
/// 用最小会被页边一条竖线带偏，用中位数在「缩进行恰好占一半」的页上会选错。
fn low_percentile(mut v: Vec<f32>, p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((v.len() as f32 - 1.0) * p).round() as usize;
    v[idx.min(v.len() - 1)]
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

/// 一页的行 → 段落。返回每段的文字，以及这段的首行原本是不是缩进的。
pub fn paragraphs_from_lines(lines: &[LineBox]) -> Vec<Para> {
    let mut out: Vec<Para> = Vec::new();
    if lines.is_empty() {
        return out;
    }
    let h = median(lines.iter().map(|l| l.height()).collect());
    // 段的左边界就是正文的左边距：取左边界的低分位数（10%），
    // 缩进行天然是少数，不会被它们抬上去。
    let para_left = low_percentile(lines.iter().map(|l| l.left).collect(), 0.1);
    let page_right = lines.iter().map(|l| l.right).fold(f32::MIN, f32::max);
    let indented = |l: &LineBox| l.left - para_left > 1.2 * h;

    let mut acc = String::new();
    // 当前这一段的首行是否缩进。开新段时按「触发切段的那一行」定：
    // 靠缩进切出来的段自然是缩进的，靠空档或短行切出来的自然不是。
    let mut acc_indented = false;
    let mut prev: Option<&LineBox> = None;

    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        match prev {
            None => acc_indented = indented(line),
            Some(p) => {
                let gap = line.top - p.bottom;
                let prev_short = page_right - p.right > 2.0 * h;
                let back_to_margin = line.left - para_left <= 0.6 * h;
                if gap > 0.8 * h || indented(line) || (prev_short && back_to_margin) {
                    out.push(Para {
                        text: std::mem::take(&mut acc),
                        first_line_indent: acc_indented,
                    });
                    acc_indented = indented(line);
                }
            }
        }
        join(&mut acc, text);
        prev = Some(line);
    }
    if !acc.is_empty() {
        out.push(Para { text: acc, first_line_indent: acc_indented });
    }
    out
}

/// 已经带文字层的一页 → 段落：空行分段，段内合并。
///
/// 文字层没有框，看不出首行缩进，所以一律不标 —— 宁可少给一个缩进，
/// 也不要把「提取时被吃掉的行首空格」当成排版。
pub fn paragraphs_from_text(text: &str) -> Vec<Para> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if !acc.is_empty() {
                out.push(Para::plain(std::mem::take(&mut acc)));
            }
            continue;
        }
        join(&mut acc, line);
    }
    if !acc.is_empty() {
        out.push(Para::plain(acc));
    }
    out
}

fn body_fonts() -> RunFonts {
    // 西文 Times New Roman、中文宋体：一份中文文档在 Word 里的常态，
    // 也是「扫描件数字化」最不喧宾夺主的组合。
    RunFonts::new().ascii("Times New Roman").east_asia("宋体")
}

/// 正文字号：12 pt（docx 的 size 单位是半磅）。
const BODY_HALF_POINTS: usize = 24;

/// 首行缩进两字，单位 twips（1 pt = 20 twips）：2 × 12 pt × 20 = 480。
///
/// 本来该用 `Paragraph::first_line_chars(200)`（百分之一字符，正是 Word 里
/// 「首行缩进 2 字符」的写法），但 docx-rs 0.4 的 `Indent::build_to` 只把
/// start / special_indent / end / start_chars 交给 XML 写入器，`first_line_chars`
/// 被标成「Internal, for reading」直接丢掉 —— 设了也不进文件，上游主干后来
/// 才把写入器扩到七参。所以这里给绝对宽度：正文 12 pt 时 480 twips 恰好
/// 两个汉字宽，Word 里显示 0.85 厘米。
const FIRST_LINE_INDENT_TWIPS: i32 = 480;

fn body(text: &str) -> Paragraph {
    Paragraph::new()
        .add_run(Run::new().size(BODY_HALF_POINTS).fonts(body_fonts()).add_text(text))
        .line_spacing(
            LineSpacing::new()
                .line(240)
                .line_rule(LineSpacingType::Auto),
        )
}

/// 写出一份 .docx。
///
/// `pages` 每页若干段。页与页之间插分页符，与原件页码一一对应 ——
/// 少了它，导出稿的页数就跟原件对不上，对稿子的人会错行。
pub fn write(
    pages: &[Vec<Para>],
    page_twips: Option<(u32, u32)>,
    output: &str,
) -> anyhow::Result<Stats> {
    // twips（1/1440 英寸），docx-rs 的 page_size 要 u32；缺省 A4 纵向
    let (w, h) = page_twips.unwrap_or((11906, 16838));
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
    for (i, page) in pages.iter().enumerate() {
        for (j, para) in page.iter().enumerate() {
            let mut p = body(&para.text);
            if para.first_line_indent {
                p = p.indent(None, Some(SpecialIndentType::FirstLine(FIRST_LINE_INDENT_TWIPS)), None, None);
            }
            // 每页第一段带分页符（首页不带）。
            if i > 0 && j == 0 {
                p = p.page_break_before(true);
            }
            doc = doc.add_paragraph(p);
            paragraphs += 1;
        }
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

    Ok(Stats { pages: pages.len(), paragraphs })
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
        assert!(p[0].text.contains("nine, and I have stopped"), "{}", p[0].text);
        assert!(!p[0].first_line_indent, "首行顶格，不该标缩进");
    }

    #[test]
    fn blank_gap_starts_a_new_paragraph() {
        let lines = vec![
            lb("First paragraph ends here.", 100.0, 50.0, 900.0),
            lb("Second paragraph starts here.", 220.0, 50.0, 780.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[0].text, "First paragraph ends here.");
        assert!(!p[1].first_line_indent, "靠空档分段，不该标缩进");
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
            lb("    Indented opening of the next paragraph.", 145.0, 140.0, 950.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[1].text, "Indented opening of the next paragraph.");
        assert!(!p[0].first_line_indent);
        assert!(p[1].first_line_indent, "靠缩进切出来的段，要标上缩进");
    }

    #[test]
    fn cjk_lines_join_without_spaces() {
        let lines = vec![
            lb("这家店九点才开门，我早就不装作", 100.0, 50.0, 950.0),
            lb("自己能睡到六点以后了。", 145.0, 50.0, 950.0),
        ];
        let p = paragraphs_from_lines(&lines);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].text, "这家店九点才开门，我早就不装作自己能睡到六点以后了。");
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
        let p: Vec<String> = paragraphs_from_text(text).into_iter().map(|x| x.text).collect();
        assert_eq!(p, vec!["line one line two", "line three"]);
    }

    #[test]
    fn empty_page_yields_no_paragraphs() {
        assert!(paragraphs_from_lines(&[]).is_empty());
        assert!(paragraphs_from_text("   \n\n").is_empty());
    }

    /// 端到端样例：用一页真实的识别结果（两段，含缩进与短行收尾）排一份
    /// .docx，写到 target/ 下。CI 把它当产物上传，用来人工看一眼排版对不对 ——
    /// 单测只能证明「能被读回」，证明不了「读起来像一篇文章」。
    #[test]
    fn sample_docx_is_written_for_inspection() {
        let page1 = vec![
            lb("The shop does not open until nine, and I have stopped", 100.0, 80.0, 900.0),
            lb("pretending I sleep past six. Coffee, the window, the", 145.0, 80.0, 900.0),
            lb("street still deciding what kind of day it wants to be.", 190.0, 80.0, 860.0),
            lb("    Wrote three hundred words about the man who came in", 280.0, 140.0, 900.0),
            lb("on Tuesday wanting a bathtub that would outlive him.", 325.0, 80.0, 880.0),
            lb("这家店九点才开门，我早就不装作自己能睡到六点以后了。", 420.0, 80.0, 890.0),
            lb("雨从八点下到四点，进来两个人，都只是看看。", 500.0, 80.0, 900.0),
        ];
        let page2 = vec![
            lb("Page two starts here and carries a single paragraph.", 100.0, 80.0, 900.0),
        ];
        let pages = vec![
            paragraphs_from_lines(&page1),
            paragraphs_from_lines(&page2),
        ];
        assert_eq!(pages[0].len(), 4, "两段英文 + 一段首行缩进 + 一段中文：{:?}", pages[0]);
        // 切段对不对之外，还得看缩进这一位有没有带下去：样张里只有第二段是
        // 原件中缩进的，写盘后 Word 里它应该显示「首行缩进 2 字符」，其余顶格。
        assert!(!pages[0][0].first_line_indent);
        assert!(pages[0][1].first_line_indent, "第二段在原件里是缩进的：{:?}", pages[0][1]);
        assert!(!pages[0][2].first_line_indent);

        let out = std::path::Path::new("target/docx-sample.docx");
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let s = write(&pages, None, out.to_str().unwrap()).expect("写样例失败");
        assert_eq!(s.pages, 2);
        assert!(out.exists());
    }

    #[test]
    fn write_produces_a_readable_docx() {
        let dir = std::env::temp_dir().join(format!("ocr-docx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.docx");
        let pages = vec![
            vec![Para::plain("page one paragraph")],
            vec![Para::plain("page two paragraph")],
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

    /// 缩进必须真的写进文件，不能只是 Para 上有个 true。
    ///
    /// 这条是踩出来的：`Paragraph::first_line_chars(200)` 看着最对，docx-rs 0.4
    /// 的 Indent::build_to 却把这个字段丢掉，于是段落标记为「该缩进」、单测在
    /// 内存里全过、导出的 Word 通篇顶格。所以要读回文件，量到 w:firstLine 才算数。
    #[test]
    fn first_line_indent_reaches_the_file() {
        let dir = std::env::temp_dir().join(format!("ocr-docx-indent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("indent.docx");
        let pages = vec![vec![
            Para::plain("顶格的一段"),
            Para { text: "缩进的一段".to_string(), first_line_indent: true },
            Para::plain("又是顶格的一段"),
        ]];
        write(&pages, None, out.to_str().unwrap()).unwrap();

        let bytes = std::fs::read(&out).unwrap();
        let doc = docx_rs::read_docx(&bytes).unwrap();
        let mut got = Vec::new();
        for child in &doc.document.children {
            if let docx_rs::DocumentChild::Paragraph(p) = child {
                got.push(
                    p.property
                        .indent
                        .as_ref()
                        .and_then(|i| i.special_indent)
                        .map(|s| matches!(s, SpecialIndentType::FirstLine(n) if n == FIRST_LINE_INDENT_TWIPS))
                        .unwrap_or(false),
                );
            }
        }
        assert_eq!(got, vec![false, true, false], "首行缩进没进文件：{got:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
