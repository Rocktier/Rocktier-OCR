// Rocktier OCR 前端：拖拽选文件，调用后端命令，订阅进度事件。
//
// 拖拽路径只在 Tauri 的 webview 拖拽事件里才是真实路径 —— 浏览器自己的
// dataTransfer 给的是披着假路径的 File 对象，所以以 onDragDropEvent 为准，
// 另外两个入口只作兜底。
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open, save } = window.__TAURI__.dialog;

const el = (id) => document.getElementById(id);
let inputPath = null;
let running = false;
// 转换完成的文档。识别只做一次，三个产物都从它出。
let job = null;

const SUPPORTED = [".pdf", ".png", ".jpg", ".jpeg"];
function isSupported(name) {
  return SUPPORTED.some((ext) => name.toLowerCase().endsWith(ext));
}

function log(msg) {
  const box = el("log");
  box.style.display = "block";
  box.textContent += msg + "\n";
  box.scrollTop = box.scrollHeight;
}

function setRunning(on) {
  running = on;
  el("go").disabled = on || !inputPath;
  el("cancel").disabled = !on;
  el("pdf").disabled = on || !job;
  el("txt").disabled = on || !job;
  el("copy").disabled = on || !job;
  el("bar-wrap").style.display = on ? "block" : "none";
  if (on) el("status").textContent = tr().preparing;
}

function setPath(p) {
  if (!p) return;
  inputPath = p;
  job = null;
  el("path").textContent = p;
  el("done").style.display = "none";
  el("go").disabled = running || !p;
  el("pdf").disabled = true;
  el("txt").disabled = true;
  el("copy").disabled = true;
}

async function pickFile() {
  try {
    const p = await open({
      multiple: false,
      filters: [
        { name: "PDF", extensions: ["pdf"] },
        { name: "Image", extensions: ["png", "jpg", "jpeg"] },
      ],
    });
    if (p) setPath(p);
  } catch (err) {
    log(tr().dialogFailed + err);
  }
}

el("drop").addEventListener("click", pickFile);
el("drop").addEventListener("dragover", (e) => {
  e.preventDefault();
  el("drop").classList.add("over");
});
el("drop").addEventListener("dragleave", () => el("drop").classList.remove("over"));
el("drop").addEventListener("drop", (e) => {
  e.preventDefault();
  el("drop").classList.remove("over");
  // 兜底：真实路径由 webview 事件给出，这里只在它缺失时尝试。
  if (inputPath) return;
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
        const paths = payload.paths || [];
        setPath(paths.find((p) => isSupported(p)) || null);
        if (!paths.length) log(tr().notFile);
      }
    });
    void unlisten;
  } catch (err) {
    log(tr().dragNotEnabled + err);
  }
}
bindDragDrop();

// 旧式事件名兜底（不同版本的 Tauri 都发过这个）。
listen("tauri://drag-drop", (e) => {
  const paths = e.payload?.paths || [];
  setPath(paths.find((p) => isSupported(p)) || null);
});

el("go").addEventListener("click", async () => {
  if (!inputPath || running) return;
  setRunning(true);
  el("done").style.display = "none";
  el("log").textContent = "";
  log(tr().startLog + inputPath);
  try {
    const s = await invoke("ocr_process", { input: inputPath, dpi: 200 });
    job = s;
    el("done").style.display = "block";
    const text = s.already_searchable ? tr().already : tr().converted(s);
    el("done").innerHTML = text;
    log(tr().finished + tr().converted(s).replace(/<[^>]+>/g, ""));
    el("status").textContent = tr().statusDone;
  } catch (err) {
    const t = tr();
    const msg = String(err);
    const wasCancelled = msg.includes("cancelled");
    log(wasCancelled ? t.cancelled : t.failed + msg);
    el("status").textContent = wasCancelled ? t.statusCancelled : t.statusFailed;
  }
  setRunning(false);
});

// 复制到剪贴板必须走原生插件：webview 跑在 tauri:// 源上，
// 既不是安全上下文（navigator.clipboard 被拒），execCommand 也被拒。
async function copyToClipboard(text) {
  await invoke("plugin:clipboard-manager|write_text", { text });
}

el("cancel").addEventListener("click", () => invoke("ocr_cancel"));

// 导出 TXT：原生页用自带文字，扫描页与图片走识别。
el("txt").addEventListener("click", async () => {
  if (!job || running) return;
  let out;
  try {
    out = await save({
      defaultPath: inputPath.replace(/\.(pdf|png|jpe?g)$/i, "") + "-text.txt",
      filters: [{ name: tr().txtName, extensions: ["txt"] }],
    });
  } catch (err) {
    log(tr().saveDialogFailed + err);
    return;
  }
  if (!out) return;
  log(tr().startLog + inputPath);
  try {
    const s = await invoke("ocr_export_txt", { output: out });
    el("done").style.display = "block";
    el("done").innerHTML = tr().exportDone(s) + `<br><span class="out">${out}</span>`;
    log(tr().exportDone(s));
  } catch (err) {
    log(tr().exportFailed + err);
    el("status").textContent = tr().statusFailed;
  }
});

// 复制全文：同一份文本进剪贴板。
el("copy").addEventListener("click", async () => {
  if (!job || running) return;
  try {
    const s = await invoke("ocr_export_txt", {});
    await copyToClipboard(s.text);
    log(tr().copied(s.chars));
    el("done").style.display = "block";
    el("done").innerHTML = tr().copied(s.chars);
  } catch (err) {
    log(tr().copyFailed + err);
    el("status").textContent = tr().statusFailed;
  }
});

// 导出可搜索 PDF：把已识别的文字层写进原文档的一份副本。
el("pdf").addEventListener("click", async () => {
  if (!job || running) return;
  let out;
  try {
    out = await save({
      defaultPath: inputPath.replace(/\.(pdf|png|jpe?g)$/i, "") + tr().saveDefault + ".pdf",
      filters: [{ name: tr().pdfName, extensions: ["pdf"] }],
    });
  } catch (err) {
    log(tr().saveDialogFailed + err);
    return;
  }
  if (!out) return;
  try {
    const r = await invoke("ocr_export_pdf", { output: out });
    el("done").style.display = "block";
    el("done").innerHTML = tr().pdfDone(r) + `<br><span class="out">${out}</span>`;
    log(tr().finished + out);
    el("status").textContent = tr().statusDone;
  } catch (err) {
    log(tr().exportFailed + err);
    el("status").textContent = tr().statusFailed;
  }
});

listen("ocr-progress", (e) => {
  const p = e.payload;
  el("status").textContent = tr().statusPage(p.page, p.total, p.phase);
  el("bar").style.width = `${((p.page / p.total) * 100).toFixed(1)}%`;
});

setRunning(false);
