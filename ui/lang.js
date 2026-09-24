// 家族惯例：界面双语（中文 / 英文），选择记在本机，不改系统设置。
const STRINGS = {
  zh: {
    title: "Rocktier OCR",
    tagline: "扫描件变可搜索",
    dropBig: "把 PDF 或图片拖到这里",
    dropSub1: "扫描件与图片型 PDF 会被识别，并加上可搜索的文字层",
    dropSub2: "图片会被包成一页可搜索的 PDF；原生电子版整页跳过，一个字符都不动",
    imageDone: (s) =>
      `已把这张图包成 <b>一页可搜索的 PDF</b>，识别出 ${s.lines} 行文本。`,
    go: "开始识别",
    cancel: "取消",
    exportTxt: "导出 TXT",
    copyText: "复制文字",
    copied: (n) => `已复制 ${n} 字符到剪贴板。`,
    exportDone: (s) => `已导出 TXT：${s.pages} 页、${s.lines} 行、${s.chars} 字符。`,
    txtName: "文本",
    copyFailed: "复制失败：",
    exportFailed: "导出失败：",
    preparing: "准备中…",
    statusPage: (p, t, phase) => `页 ${p}/${t} · ${phase}`,
    done: (s) =>
      `完成：<b>${s.pages_ocr} 页</b>识别加层，${s.pages_skipped} 页原生跳过，共 ${s.lines} 行文本。`,
    already:
      "这份 PDF 本来就可搜索 —— 每一页都已带有文字层（通常是 Word/Excel 导出的），没有需要识别的像素，所以原样保留，一个字符都没有动。",
    alreadyLog: "该文档已含文字层，无需识别。",
    output: "输出",
    finished: "完成 ✅ 输出：",
    cancelled: "已取消。",
    failed: "失败：",
    statusCancelled: "已取消",
    statusFailed: "失败",
    statusDone: "完成",
    footer: "识别在本机完成，文档不会离开这台电脑。",
    dialogFailed: "打开文件对话框失败：",
    saveDialogFailed: "打开保存对话框失败：",
    notFile: "拖入的不是文件，或系统未提供路径。",
    dragNotEnabled: "拖拽监听未启用：",
    startLog: "开始识别：",
    saveDefault: "-searchable",
  },
  en: {
    title: "Rocktier OCR",
    tagline: "Make scans searchable",
    dropBig: "Drop a PDF or an image here",
    dropSub1: "Scans and image-only PDFs are recognised and given a searchable text layer",
    dropSub2: "An image is wrapped into a one-page searchable PDF; born-digital pages are skipped untouched",
    imageDone: (s) =>
      `Wrapped the image into <b>a one-page searchable PDF</b> with ${s.lines} lines of text.`,
    go: "Recognise",
    cancel: "Cancel",
    exportTxt: "Export TXT",
    copyText: "Copy text",
    copied: (n) => `Copied ${n} characters to the clipboard.`,
    exportDone: (s) => `TXT exported: ${s.pages} page(s), ${s.lines} lines, ${s.chars} characters.`,
    txtName: "Text",
    copyFailed: "Copy failed: ",
    exportFailed: "Export failed: ",
    preparing: "Preparing…",
    statusPage: (p, t, phase) => `Page ${p}/${t} · ${phase}`,
    done: (s) =>
      `Done: <b>${s.pages_ocr} page(s)</b> recognised and layered, ${s.pages_skipped} skipped, ${s.lines} lines of text.`,
    already:
      "This PDF is already searchable — every page carries its own text layer (typically exported from Word or Excel). There are no pixels to recognise, so nothing was changed.",
    alreadyLog: "The document already has a text layer; no recognition needed.",
    output: "Output",
    finished: "Done ✅ output: ",
    cancelled: "Cancelled.",
    failed: "Failed: ",
    statusCancelled: "Cancelled",
    statusFailed: "Failed",
    statusDone: "Done",
    footer: "Recognition happens on this machine. Your document never leaves it.",
    dialogFailed: "Could not open the file dialog: ",
    saveDialogFailed: "Could not open the save dialog: ",
    notFile: "That drop was not a file, or the system gave no path.",
    dragNotEnabled: "Drag-and-drop listener unavailable: ",
    startLog: "Recognising: ",
    saveDefault: "-searchable",
  },
};

const LANG_KEY = "rocktier-ocr-lang";
let current =
  localStorage.getItem(LANG_KEY) ||
  (navigator.language.startsWith("zh") ? "zh" : "en");

function tr() {
  return STRINGS[current] || STRINGS.en;
}

function applyLang() {
  const t = tr();
  document.querySelectorAll("[data-i18n]").forEach((node) => {
    const key = node.getAttribute("data-i18n");
    if (t[key] !== undefined) node.innerHTML = t[key];
  });
  const toggle = document.getElementById("lang");
  if (toggle) toggle.textContent = current === "zh" ? "EN" : "中文";
  localStorage.setItem(LANG_KEY, current);
}

function toggleLang() {
  current = current === "zh" ? "en" : "zh";
  applyLang();
}

document.addEventListener("DOMContentLoaded", () => {
  applyLang();
  const toggle = document.getElementById("lang");
  if (toggle) toggle.addEventListener("click", toggleLang);
});
