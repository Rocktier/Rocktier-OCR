/* 语言访问已统一到 ui/i18n.js 的 t(path, vars)（2026-11-19，与 PDF 仓同构） */
/* 语言访问已统一到 ui/i18n.js 的 t(path, vars)（2026-11-19，与 PDF 仓同构） */
/* 语言访问已统一到 ui/i18n.js 的 t(path, vars)（2026-11-19，与 PDF 仓同构） */
// Rocktier OCR 前端：拖拽选文件，调用后端命令，订阅进度事件。
//
// 拖拽路径只在 Tauri 的 webview 拖拽事件里才是真实路径 —— 浏览器自己的
// dataTransfer 给的是披着假路径的 File 对象，所以以 onDragDropEvent 为准，
// 另外两个入口只作兜底。
//
// 界面只有一个状态源 ui.phase：顶栏、主区、状态栏的字都从它推出来，语言切换
// 后整屏重绘，不会留下「切了语言还停在上一种语言的进度句」。
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open, save } = window.__TAURI__.dialog;

const el = (id) => document.getElementById(id);
let inputPath = null;
let running = false;
// 转换完成的文档。识别只做一次，三个产物都从它出。
let job = null;
// 真实落盘路径：只有导出成功后才存在，界面上不预填、不猜。
let outPath = null;

// phase: idle（未选文件）| ready（已选）| running | done | cancelled | failed
const ui = { phase: "idle", page: 0, total: 0, error: "" };

const SUPPORTED = [".pdf", ".png", ".jpg", ".jpeg"];
function isSupported(name) {
  return SUPPORTED.some((ext) => name.toLowerCase().endsWith(ext));
}

function baseName(p) {
  return (p || "").split(/[\\/]/).pop() || p || "";
}

// invoke 的 Err 可能是字符串、Error 实例或序列化对象；对对象直接拼接只会
// 得到 "[object Object]"（真实事故：导出 PDF 失败时用户日志里只剩这行）。
function errMsg(e) {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return (e && e.message) || String(e);
}

function log(msg) {
  el("log-wrap").hidden = false;
  const box = el("log");
  box.textContent += msg + "\n";
  box.scrollTop = box.scrollHeight;
}

// ── 状态栏文字：进行中说进度，结束说结果，失败说原因 ───────────────────
function statusText() {
    switch (ui.phase) {
    case "running":
      // 后端只在有真实页号时才发页事件；preparing 阶段没有页号就不编。
      return ui.total && ui.page ? t.statusProgress(ui.page, ui.total) : t.preparing;
    case "done":
      if (!job) return t.statusDone;
      return job.already_searchable ? t.statusAlready : t.statusSummary(job);
    case "failed":
      return t.failed + ui.error;
    case "cancelled":
      return t.statusCancelled;
    case "ready":
      return t.statusReady;
    default:
      return t.statusIdle;
  }
}

// ── 整屏重绘（同时是语言切换后的重绘入口）──────────────────────────────
function render() {
    const isRunning = ui.phase === "running";
  const canExport = !isRunning && !!job;

  const status = el("status");
  status.textContent = statusText();
  status.title = status.textContent;

  // 红点只在识别进行中出现，且旁边一定有进度文字（家族 §一.2）
  el("live").hidden = !isRunning;
  el("bar-wrap").hidden = !isRunning;
  const pct = isRunning ? (ui.total ? Math.min((ui.page / ui.total) * 100, 99) : 0) : 0;
  el("bar").style.width = `${pct.toFixed(1)}%`;

  el("cancel").disabled = !isRunning;
  el("go").disabled = isRunning || !inputPath;
  el("pick").disabled = isRunning;
  el("pdf").disabled = !canExport;
  el("docx").disabled = !canExport;
  el("txt").disabled = !canExport;
  el("copy").disabled = !canExport;

  el("pick-label").textContent = inputPath ? t.changeFile : t.pickFile;
  el("pick").title = inputPath ? t.changeFile : t.pickFile;

  // 结果摘要：只在真有识别结果时出现，计数全部来自 ocr_process 返回值
  el("result").hidden = !job;
  if (job) {
    el("result-file").textContent = baseName(inputPath);
    el("result-file").title = inputPath || "";
    el("stat-total").textContent = job.pages_total;
    el("stat-ocr").textContent = job.pages_ocr;
    el("stat-skip").textContent = job.pages_skipped;
    el("stat-lines").textContent = job.lines;
    el("done").innerHTML = job.already_searchable
      ? t.already
      : job.kind === "image"
        ? t.imageDone(job)
        : t.recognised(job);

    const out = el("out-path");
    if (outPath) {
      out.textContent = outPath;
      out.title = outPath;
      out.removeAttribute("data-state");
    } else {
      out.textContent = t.outputPending;
      out.removeAttribute("title");
      out.setAttribute("data-state", "empty");
    }
  }
}

// ── 选文件 ────────────────────────────────────────────────────────────
function setPath(p) {
  if (!p) return;
  inputPath = p;
  job = null;
  outPath = null;
  ui.phase = "ready";
  ui.page = 0;
  ui.total = 0;
  ui.error = "";
  el("file-name").textContent = baseName(p);
  el("path").textContent = p;
  el("file").hidden = false;
  el("log").textContent = "";
  el("log-wrap").hidden = true;
  render();
}

async function pickFile() {
  if (running) return;
  try {
    const p = await open({
      multiple: false,
      filters: [
        { name: "PDF & Images", extensions: ["pdf", "png", "jpg", "jpeg"] },
        { name: "PDF", extensions: ["pdf"] },
        { name: "Images", extensions: ["png", "jpg", "jpeg"] },
      ],
    });
    if (p) setPath(p);
  } catch (err) {
    log(t('dialogFailed') + errMsg(err));
  }
}

el("pick").addEventListener("click", pickFile);

el("drop").addEventListener("click", pickFile);
// role="button" 要能用键盘触发（Enter / Space）
el("drop").addEventListener("keydown", (e) => {
  if (e.key === "Enter" || e.key === " " || e.key === "Spacebar") {
    e.preventDefault();
    pickFile();
  }
});
el("drop").addEventListener("dragover", (e) => {
  e.preventDefault();
  el("drop").classList.add("over");
});
el("drop").addEventListener("dragleave", () => el("drop").classList.remove("over"));
el("drop").addEventListener("drop", (e) => {
  e.preventDefault();
  el("drop").classList.remove("over");
  // 兜底：真实路径由 webview 事件给出，这里只在它缺失时尝试。
  if (inputPath || running) return;
  const files = e.dataTransfer?.files;
  if (files && files.length && isSupported(files[0].name)) {
    setPath(files[0].path || null);
  }
});

// 主入口：Tauri 2 的 webview 拖拽事件带真实路径。
async function bindDragDrop() {
  const webview = window.__TAURI__?.webview;
  if (!webview?.getCurrentWebview) return;
  try {
    const unlisten = await webview.getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload || {};
      const type = payload.type || event.type || "";
      if (type === "enter" || type === "over") {
        el("drop").classList.add("over");
        return;
      }
      if (type === "leave") {
        el("drop").classList.remove("over");
        return;
      }
      if (type === "drop") {
        el("drop").classList.remove("over");
        // 识别进行中换文件会让展示结果与正在处理的内容对不上，直接忽略。
        if (running) return;
        const paths = payload.paths || [];
        setPath(paths.find((p) => isSupported(p)) || null);
        if (!paths.length) log(t('notFile'));
      }
    });
    void unlisten;
  } catch (err) {
    log(t('dragNotEnabled') + errMsg(err));
  }
}
bindDragDrop();

// 旧式事件名兜底（不同版本的 Tauri 都发过这个）。
listen("tauri://drag-drop", (e) => {
  if (running) return;
  const paths = e.payload?.paths || [];
  setPath(paths.find((p) => isSupported(p)) || null);
});

// 文件关联（双击 / 「打开方式」）：冷启动路径经 initial_file 拉取，热启动由
// Rust 的 open-file 事件送达。setPath 是 pickFile / 拖放共用的同一条装载
// 路径（含界面状态重置），复用即可；isSupported 过滤与拖放同一判据。
invoke("initial_file")
  .then((p) => { if (p && isSupported(p)) setPath(p); })
  .catch(() => {});
listen("open-file", (e) => {
  if (running) return;
  const p = e?.payload;
  if (p && isSupported(p)) setPath(p);
});

// 原生菜单（家族规范第十三章）：挂载即按当前语言构建；applyLang 是挂在
// window 上的函数声明，包一层即可在语言切换时重建（与 index.html 主题脚本
// 的包装法一致，两层包装互不干扰）。
function buildMenu() {
  invoke("build_menu", { lang: getLang() }).catch(() => {});
}
buildMenu();
const origApplyLang = window.applyLang;
window.applyLang = function () {
  try {
    document.documentElement.lang =
      localStorage.getItem("rocktier.lang") || localStorage.getItem("rocktier-ocr-lang") || "en";
  } catch (e) { /* ignore */ }
  if (typeof origApplyLang === "function") origApplyLang();
  buildMenu();
  afterLangChange();
};

// 语言切换后：状态栏 / 结果摘要 / 顶栏按钮文案都按新语言重绘
function afterLangChange() {
  render();
}

// 菜单自定义项 → 现有动作链。导出项直接点对应按钮：禁用态与"运行中"
// 守卫都由按钮自身的逻辑把关，菜单项无需单独维护可用性。
listen("menu-action", (e) => {
  switch (e.payload) {
    case "open":
      pickFile();
      break;
    case "export-docx":
      el("docx").click();
      break;
    case "export-txt":
      el("txt").click();
      break;
    case "export-pdf":
      el("pdf").click();
      break;
    case "toggle-lang":
      toggleLang();
      break;
    case "toggle-theme":
      el("rocktier.theme").click();
      break;
    case "website":
      void invoke("open_url", { url: "https://rocktier.com/" }).catch(() => {});
      break;
    case "feedback":
      void invoke("open_url", { url: "mailto:hello@rocktier.com" }).catch(() => {});
      break;
    case "check-updates":
      if (window.ocrRunUpdateCheck) window.ocrRunUpdateCheck();
      break;
  }
});

// ── 唯一主操作：识别文字 ───────────────────────────────────────────────
el("go").addEventListener("click", async () => {
  if (!inputPath || running) return;
  running = true;
  job = null;
  outPath = null;
  ui.phase = "running";
  ui.page = 0;
  ui.total = 0;
  ui.error = "";
  el("log").textContent = "";
  el("log-wrap").hidden = true;
  render();
  log(t('startLog') + inputPath);
  try {
    const s = await invoke("ocr_process", { input: inputPath, dpi: 200 });
    job = s;
    ui.phase = "done";
    log(t('recognisedLog', s));
  } catch (err) {
        const msg = errMsg(err);
    const wasCancelled = msg.includes("cancelled");
    ui.phase = wasCancelled ? "cancelled" : "failed";
    ui.error = wasCancelled ? "" : msg;
    log(wasCancelled ? t.cancelled : t.failed + msg);
  }
  running = false;
  render();
});

// 复制到剪贴板必须走原生插件：webview 跑在 tauri:// 源上，
// 既不是安全上下文（navigator.clipboard 被拒），execCommand 也被拒。
async function copyToClipboard(text) {
  await invoke("plugin:clipboard-manager|write_text", { text });
}

el("cancel").addEventListener("click", () => invoke("ocr_cancel"));

// 导出 Word：识别结果直接落成 .docx。扫描件给别人改稿时最常用的一条路。
el("docx").addEventListener("click", async () => {
  if (!job || running) return;
  let out;
  try {
    out = await save({
      defaultPath: inputPath.replace(/\.(pdf|png|jpe?g)$/i, "") + t('docxDefault') + ".docx",
      filters: [{ name: t('docxName'), extensions: ["docx"] }],
    });
  } catch (err) {
    log(t('saveDialogFailed') + errMsg(err));
    return;
  }
  if (!out) return;
  try {
    const r = await invoke("ocr_export_docx", { output: out });
    outPath = out;
    log(t('finished') + out);
    render();
    el("status").innerHTML = t('docxDone', r);
  } catch (err) {
    log(t('exportFailed') + errMsg(err));
    el("status").textContent = t('failed') + errMsg(err);
  }
});

// 导出 TXT：原生页用自带文字，扫描页与图片走识别。
el("txt").addEventListener("click", async () => {
  if (!job || running) return;
  let out;
  try {
    out = await save({
      defaultPath: inputPath.replace(/\.(pdf|png|jpe?g)$/i, "") + "-text.txt",
      filters: [{ name: t('txtName'), extensions: ["txt"] }],
    });
  } catch (err) {
    log(t('saveDialogFailed') + errMsg(err));
    return;
  }
  if (!out) return;
  log(t('startLog') + inputPath);
  try {
    const s = await invoke("ocr_export_txt", { output: out });
    outPath = out;
    log(t('exportDone', s));
    render();
    el("status").innerHTML = t('exportDone', s);
  } catch (err) {
    log(t('exportFailed') + errMsg(err));
    ui.error = errMsg(err);
    el("status").textContent = t('failed') + ui.error;
  }
});

// 复制全文：同一份文本进剪贴板。
el("copy").addEventListener("click", async () => {
  if (!job || running) return;
  try {
    const s = await invoke("ocr_export_txt", {});
    await copyToClipboard(s.text);
    log(t('copied', s.chars));
    el("status").textContent = t('copied', s.chars);
  } catch (err) {
    log(t('copyFailed') + errMsg(err));
    el("status").textContent = t('failed') + errMsg(err);
  }
});

// 导出可搜索 PDF：把已识别的文字层写进原文档的一份副本。
el("pdf").addEventListener("click", async () => {
  if (!job || running) return;
  let out;
  try {
    out = await save({
      defaultPath: inputPath.replace(/\.(pdf|png|jpe?g)$/i, "") + t('saveDefault') + ".pdf",
      filters: [{ name: t('pdfName'), extensions: ["pdf"] }],
    });
  } catch (err) {
    log(t('saveDialogFailed') + errMsg(err));
    return;
  }
  if (!out) return;
  try {
    const r = await invoke("ocr_export_pdf", { output: out });
    outPath = out;
    log(t('finished') + out);
    render();
    el("status").innerHTML = t('pdfDone', r);
  } catch (err) {
    log(t('exportFailed') + errMsg(err));
    el("status").textContent = t('failed') + errMsg(err);
  }
});

// 进度事件：page / total / phase 都是后端真实值，界面只做映射，不做估算。
listen("ocr-progress", (e) => {
  if (ui.phase !== "running") return;
  const p = e.payload || {};
  ui.page = Number(p.page) || 0;
  ui.total = Number(p.total) || 0;
  render();
});

render();
