// 家族语言标准（2026-10-13 收敛）：en / zh 两门。原 8 语里的非中英为未审校机翻基线，
// 按家族裁定移除（family.json globalRules.$locales_2026_10_13）。
// 选择记在本机（rocktier.lang），不改系统设置；无偏好时默认英文，不嗅探系统语言。
const STRINGS = {
  zh: {
    title: "Rocktier OCR",
    tagline: "扫描件转文字",
    formats: "PDF · PNG · JPG",
    dropBig: "把 PDF 或图片拖到这里",
    dropSub1: "扫描件与图片型 PDF 会被识别，并加上可搜索的文字层",
    dropSub2: "图片会被包成一页可搜索的 PDF；已含文字层的页面整页跳过",
    pickFile: "选择文件",
    changeFile: "换一份文件",
    goHint: "已含文字层的页面整页跳过，不重复识别",
    resultTitle: "识别结果",
    statPages: "页数",
    statOcr: "识别页",
    statSkipped: "跳过页",
    statLines: "行数",
    outputLabel: "输出",
    outputPending: "尚未写入 —— 导出后这里是保存路径",
    exportLabel: "导出",
    details: "详情",
    statusIdle: "选择一份 PDF 或图片开始",
    statusReady: "就绪 —— 点「识别文字」开始",
    statusProgress: (p, t) => `识别中 · 第 ${p} 页 / 共 ${t} 页`,
    statusSummary: (s) =>
      `完成 · 共 ${s.pages_total} 页 · 识别 ${s.pages_ocr} 页 · 跳过 ${s.pages_skipped} 页 · ${s.lines} 行`,
    statusAlready: "本来就带文字层，无需识别",
    offlineNote: "离线 · 文件不出本机",
    imageDone: (s) =>
      `这张图会包成一页可搜索的 PDF，识别出 ${s.lines} 行文本。还没落盘 —— 用下面的按钮导出。`,
    start: "识别文字",
    exportPdf: "导出 PDF",
    cancel: "取消",
    exportTxt: "导出 TXT",
    copyText: "复制文本",
    copied: (n) => `已复制 ${n} 字符到剪贴板。`,
    exportDone: (s) => `已导出 TXT：${s.pages} 页、${s.lines} 行、${s.chars} 字符。`,
    txtName: "文本",
    pdfName: "PDF 文档",
    recognised: () =>
      "识别完成 —— 需要识别的页面都已加上文字层。还没落盘，用下面的按钮导出。",
    recognisedLog: (s) =>
      `识别完成：${s.pages_ocr} 页加层、${s.pages_skipped} 页原生跳过，${s.lines} 行文本。`,
    pdfDone: (r) => `已导出可搜索 PDF：<b>${r.pages} 页</b>。`,
    copyFailed: "复制失败：",
    exportFailed: "导出失败：",
    preparing: "准备中…",
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
      buy: "购买 — $14.99",
      close: "关闭",
      invalid: "该激活码未被接受。请检查是否输错（不区分大小写）。",
      wrongProduct: "这个激活码属于另一个 Rocktier 应用。每个应用各有自己的码，或者用全家桶（可解锁全部）。",
      refunded: "这个激活码对应的购买已退款，因此不能再解锁。如属误判，请把订单号发到 hello@rocktier.com。",
      offline: "连不上 rocktier.com。激活需要一次联网，之后便不再联网。",
      deviceLimit: "这张激活码的设备数已达上限。请先到 rocktier.com/license 解绑一台设备，再重试。",
      whereToFind: "付款后页面上会显示激活码，购买确认邮件里也有一份。",
      privacyNote: "激活会把激活码发送到 rocktier.com 一次，并把签名回执保存在本机。除此之外不传输任何内容。",
      exportHintTrial: (d) => `导出 PDF/TXT 需要授权 —— 试用还剩 ${d} 天。`,
      exportHintExpired: "试用已结束，导出 PDF/TXT 需要先激活。",
    },
    update: {
      check: "检查更新",
      checking: "正在检查…",
      upToDate: "已是最新版本",
      avail: "有新版本：",
      failed: "检查失败，请稍后再试",
      get: "获取",
      privacy: "点击才会联网，应用不会自动检查",
      lastPrefix: "上次检查：",
      never: "从未检查",
      close: "关闭",
      openReleases: "打开发布页",
      openReleasesHint: "也可以直接打开发布页查看。",
    },
  },
  en: {
    title: "Rocktier OCR",
    tagline: "Scans into text",
    formats: "PDF · PNG · JPG",
    dropBig: "Drop a PDF or an image",
    dropSub1: "Scans and image-only PDFs get a searchable text layer",
    dropSub2: "An image becomes a one-page searchable PDF; pages that already have text are skipped",
    pickFile: "Choose file",
    changeFile: "Replace file",
    goHint: "Pages that already carry text are skipped, never overwritten",
    resultTitle: "Result",
    statPages: "Pages",
    statOcr: "Recognised",
    statSkipped: "Skipped",
    statLines: "Lines",
    outputLabel: "Output",
    outputPending: "Not written yet — export to save",
    exportLabel: "Export",
    details: "Details",
    statusIdle: "Choose a PDF or an image to start",
    statusReady: "Ready — press Recognise text",
    statusProgress: (p, t) => `Recognising · page ${p} of ${t}`,
    statusSummary: (s) =>
      `Done · ${s.pages_total} page(s) · ${s.pages_ocr} recognised · ${s.pages_skipped} skipped · ${s.lines} lines`,
    statusAlready: "Already searchable — nothing to recognise",
    offlineNote: "Offline · your file stays on this device",
    imageDone: (s) =>
      `This image will be wrapped into a one-page searchable PDF with ${s.lines} lines of text. Nothing written yet — export below.`,
    start: "Recognise text",
    exportPdf: "Export PDF",
    cancel: "Cancel",
    exportTxt: "Export TXT",
    copyText: "Copy text",
    copied: (n) => `Copied ${n} characters to the clipboard.`,
    exportDone: (s) => `TXT exported: ${s.pages} page(s), ${s.lines} lines, ${s.chars} characters.`,
    txtName: "Text",
    pdfName: "PDF document",
    recognised: () =>
      "Done — every page that needed one now has a text layer. Nothing written yet; export below.",
    recognisedLog: (s) =>
      `Recognised: ${s.pages_ocr} page(s) layered, ${s.pages_skipped} skipped, ${s.lines} lines of text.`,
    pdfDone: (r) => `Searchable PDF exported: <b>${r.pages} page(s)</b>.`,
    copyFailed: "Copy failed: ",
    exportFailed: "Export failed: ",
    preparing: "Preparing…",
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
      buy: "Buy — $14.99",
      close: "Close",
      invalid: "That code was not accepted. Check it for a typo — the code is not case-sensitive.",
      wrongProduct: "That code belongs to a different Rocktier app. Each app has its own code — or the family bundle, which unlocks all of them.",
      refunded: "That code was refunded, so it no longer unlocks anything. If this is a mistake, write to hello@rocktier.com with your order number.",
      offline: "Could not reach rocktier.com. Activating needs one connection; after that the app stays offline.",
      deviceLimit: "This code has reached its device limit. Deactivate a device at rocktier.com/license, then try again.",
      whereToFind: "Your code was shown on the page right after payment, and is in the purchase email too.",
      privacyNote: "Activating sends the code to rocktier.com once and stores the signed reply locally. Nothing else is sent.",
      exportHintTrial: (d) => `Exporting PDF/TXT needs a license — ${d} day(s) left in your trial.`,
      exportHintExpired: "Your trial has ended — activate to export PDF/TXT.",
    },
    update: {
      check: "Check for updates",
      checking: "Checking…",
      upToDate: "You're up to date",
      avail: "New version available:",
      failed: "Couldn't check for updates",
      get: "Get",
      privacy: "Only checks when you click — the app never checks on its own",
      lastPrefix: "Last checked: ",
      never: "Never",
      close: "Close",
      openReleases: "Open releases",
      openReleasesHint: "You can also open the releases page directly.",
    },
  },
};

/* 2026-10-04 键改名 → "rocktier.lang"，与 MD / Write 同名同形。
 * 语言是用户感知最强的偏好，重置一次就等于「中文用户变英文界面」，故读取处回落旧键。 */
const LANG_KEY = "rocktier.lang";
const LANG_KEY_LEGACY = "rocktier-ocr-lang";
// 家族语言标准（2026-10-13 收敛）：en / zh 两门循环；无保存偏好时默认英文，不嗅探系统语言。
const LANG_ORDER = ["en", "zh"];
const LANG_NAMES = {
  en: "EN", zh: "中文",
};
const _savedLang = localStorage.getItem(LANG_KEY) ?? localStorage.getItem(LANG_KEY_LEGACY);
let current = LANG_ORDER.includes(_savedLang) ? _savedLang : "en";

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
  if (toggle) {
    const next = LANG_ORDER[(LANG_ORDER.indexOf(current) + 1) % LANG_ORDER.length];
    toggle.textContent = LANG_NAMES[next];
    toggle.title = LANG_NAMES[current];
  }
  localStorage.setItem(LANG_KEY, current);
}

function toggleLang() {
  current = LANG_ORDER[(LANG_ORDER.indexOf(current) + 1) % LANG_ORDER.length];
  applyLang();
}

document.addEventListener("DOMContentLoaded", () => {
  applyLang();
  const toggle = document.getElementById("rocktier.lang");
  if (toggle) toggle.addEventListener("click", toggleLang);
});
