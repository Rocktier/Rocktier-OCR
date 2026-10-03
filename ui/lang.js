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
    start: "开始转换",
    exportPdf: "导出 PDF",
    cancel: "取消",
    exportTxt: "导出 TXT",
    copyText: "复制文本",
    copied: (n) => `已复制 ${n} 字符到剪贴板。`,
    exportDone: (s) => `已导出 TXT：${s.pages} 页、${s.lines} 行、${s.chars} 字符。`,
    txtName: "文本",
    pdfName: "PDF 文档",
    converted: (s) =>
      `转换完成：<b>${s.pages_total} 页</b>（识别 ${s.pages_ocr} 页、原生跳过 ${s.pages_skipped} 页），${s.lines} 行。`,
    pdfDone: (r) => `已导出可搜索 PDF：<b>${r.pages} 页</b>。`,
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
    startLog: "开始处理：",
    saveDefault: "-searchable",
    themeToLight: "切换到浅色",
    themeToDark: "切换到深色",
    themeLabel: "主题",
    themeModeAuto: "跟随系统",
    themeModeLight: "浅色",
    themeModeDark: "深色",
    dialog: "对话框",
  unavailable: "不可用",
  Processing: "处理中",
  Failed: "失败",
    // ── 许可与激活（家族 L6；文案照 Pic2WebP 4f669ac，导出语义按 OCR 改写）──
    license: {
      title: "许可与激活",
      loading: "正在检查…",
      trialLeft: (d) => `免费试用中 —— 还剩 ${d} 天。`,
      trialChip: (d) => `试用 · ${d} 天`,
      expiredChip: "未激活",
      expired: "试用已结束。识别仍可用；导出 PDF/TXT 需要激活。",
      licensed: "已激活。谢谢。",
      licensedFamily: "已激活 —— 全家桶，所有 Rocktier 应用均已解锁。",
      licensedNote: "此副本已激活。此后不再有任何校验，也不联网。",
      storeNote: "此副本购自微软商店，许可由商店负责。",
      notConfigured: "此构建尚未配置验签公钥，暂时无法激活。请写信到 hello@rocktier.com。",
      codeLabel: "激活码",
      codePlaceholder: "RKT-…",
      activate: "激活",
      activating: "正在激活…",
      buy: "购买 — $4.99",
      close: "关闭",
      invalid: "该激活码未被接受。请检查是否输错（不区分大小写）。",
      wrongProduct: "这个激活码属于另一个 Rocktier 应用。每个应用各有自己的码，或者用全家桶（可解锁全部）。",
      refunded: "这个激活码对应的购买已退款，因此不能再解锁。如属误判，请把订单号发到 hello@rocktier.com。",
      offline: "连不上 rocktier.com。激活需要一次联网，之后便不再联网。",
      whereToFind: "付款后页面上会显示激活码，购买确认邮件里也有一份。",
      privacyNote: "激活会把激活码发送到 rocktier.com 一次，并把签名回执保存在本机。除此之外不传输任何内容。",
      // 识别完成提示：试用内说明「导出需授权」；已激活不提示（见 license.js）。
      exportHintTrial: (d) => `导出 PDF/TXT 需要授权 —— 试用还剩 ${d} 天。`,
      exportHintExpired: "试用已结束，导出 PDF/TXT 需要先激活。",
    },
},
  en: {
    title: "Rocktier OCR",
    tagline: "Make scans searchable",
    dropBig: "Drop a PDF or an image here",
    dropSub1: "Scans and image-only PDFs are recognised and given a searchable text layer",
    dropSub2: "An image is wrapped into a one-page searchable PDF; born-digital pages are skipped untouched",
    imageDone: (s) =>
      `Wrapped the image into <b>a one-page searchable PDF</b> with ${s.lines} lines of text.`,
    start: "Convert",
    exportPdf: "Export PDF",
    cancel: "Cancel",
    exportTxt: "Export TXT",
    copyText: "Copy text",
    copied: (n) => `Copied ${n} characters to the clipboard.`,
    exportDone: (s) => `TXT exported: ${s.pages} page(s), ${s.lines} lines, ${s.chars} characters.`,
    txtName: "Text",
    pdfName: "PDF document",
    converted: (s) =>
      `Converted: <b>${s.pages_total} page(s)</b> - ${s.pages_ocr} recognised, ${s.pages_skipped} already readable, ${s.lines} lines.`,
    pdfDone: (r) => `Searchable PDF exported: <b>${r.pages} page(s)</b>.`,
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
    startLog: "Processing: ",
    saveDefault: "-searchable",
    themeToLight: "Switch to light",
    themeToDark: "Switch to dark",
    themeLabel: "Theme",
    themeModeAuto: "Follow system",
    themeModeLight: "Light",
    themeModeDark: "Dark",
    // ── License: trial & activation (family L6; wording per Pic2WebP 4f669ac,
    //    the expired line rewritten for OCR: recognition still works, export needs a license)
    license: {
      title: "License",
      loading: "Checking…",
      trialLeft: (d) => `Free trial — ${d} day(s) left.`,
      trialChip: (d) => `Trial · ${d}d`,
      expiredChip: "Not activated",
      expired: "Your trial has ended. Recognition still works; exporting PDF/TXT needs a license.",
      licensed: "Licensed. Thank you.",
      licensedFamily: "Licensed — family bundle. Every Rocktier app is unlocked.",
      licensedNote: "This copy is activated. No further checks, and no network access.",
      storeNote: "This copy came from the Microsoft Store, so the Store handles the license for it.",
      notConfigured: "This build cannot activate a code yet — it carries no verification key. Please write to hello@rocktier.com.",
      codeLabel: "Activation code",
      codePlaceholder: "RKT-…",
      activate: "Activate",
      activating: "Activating…",
      buy: "Buy — $4.99",
      close: "Close",
      invalid: "That code was not accepted. Check it for a typo — the code is not case-sensitive.",
      wrongProduct: "That code belongs to a different Rocktier app. Each app has its own code — or the family bundle, which unlocks all of them.",
      refunded: "That code was refunded, so it no longer unlocks anything. If this is a mistake, write to hello@rocktier.com with your order number.",
      offline: "Could not reach rocktier.com. Activating needs one connection; after that the app stays offline.",
      whereToFind: "Your code was shown on the page right after payment, and is in the purchase email too.",
      privacyNote: "Activating sends the code to rocktier.com once and stores the signed reply locally. Nothing else is sent.",
      // Recognition-finished hint: trials see it once per conversion; licensed copies never do.
      exportHintTrial: (d) => `Exporting PDF/TXT needs a license — ${d} day(s) left in your trial.`,
      exportHintExpired: "Your trial has ended — activate to export PDF/TXT.",
    },
  },
};

const LANG_KEY = "rocktier-ocr-lang";
// 家族约定：无保存偏好时默认英文，不嗅探系统语言（与 Pic2Webp 一致）
const _savedLang = localStorage.getItem(LANG_KEY);
let current = _savedLang === "zh" || _savedLang === "en" ? _savedLang : "en";

function tr() {
  return STRINGS[current] || STRINGS.en;
}

function applyLang() {
  const t = tr();
  document.querySelectorAll("[data-i18n]").forEach((node) => {
    const key = node.getAttribute("data-i18n");
    if (t[key] !== undefined) node.innerHTML = t[key];
  });
  // 无障碍底线：屏幕阅读器要用对应语言的语音引擎
  document.documentElement.lang = current;
  const toggle = document.getElementById("rocktier.lang");
  if (toggle) toggle.textContent = current === "zh" ? "EN" : "中文";
  localStorage.setItem(LANG_KEY, current);
}

function toggleLang() {
  current = current === "zh" ? "en" : "zh";
  applyLang();
}

document.addEventListener("DOMContentLoaded", () => {
  applyLang();
  const toggle = document.getElementById("rocktier.lang");
  if (toggle) toggle.addEventListener("click", toggleLang);
});
