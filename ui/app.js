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
  el("bar-wrap").style.display = on ? "block" : "none";
  if (on) el("status").textContent = tr().preparing;
}

function setPath(p) {
  if (!p) return;
  inputPath = p;
  el("path").textContent = p;
  el("go").disabled = running || !p;
}

async function pickFile() {
  try {
    const p = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
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
  if (files && files.length && files[0].name.toLowerCase().endsWith(".pdf")) {
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
        setPath(paths.find((p) => p.toLowerCase().endsWith(".pdf")) || null);
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
  setPath(paths.find((p) => p.toLowerCase().endsWith(".pdf")) || null);
});

el("go").addEventListener("click", async () => {
  if (!inputPath || running) return;
  const stem = inputPath.replace(/\.pdf$/i, "");
  let out;
  try {
    out = await save({
      defaultPath: stem + tr().saveDefault + ".pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
  } catch (err) {
    log(tr().saveDialogFailed + err);
    return;
  }
  if (!out) return;
  setRunning(true);
  el("done").style.display = "none";
  el("log").textContent = "";
  log(tr().startLog + inputPath);
  try {
    const s = await invoke("ocr_process", { input: inputPath, output: out, dpi: 200 });
    el("done").style.display = "block";
    if (s.already_searchable) {
      el("done").innerHTML = tr().already + `<br><span class="out">${tr().output}: ${s.output}</span>`;
      log(tr().alreadyLog);
    } else {
      el("done").innerHTML = tr().done(s) + `<br><span class="out">${s.output}</span>`;
      log(tr().finished + s.output);
    }
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

el("cancel").addEventListener("click", () => invoke("ocr_cancel"));

listen("ocr-progress", (e) => {
  const p = e.payload;
  el("status").textContent = tr().statusPage(p.page, p.total, p.phase);
  el("bar").style.width = `${((p.page / p.total) * 100).toFixed(1)}%`;
});

setRunning(false);
