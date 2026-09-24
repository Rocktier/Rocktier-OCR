// Rocktier OCR 的前端逻辑：拖拽选文件，调后端命令，订阅进度事件。
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
}

async function pickFile() {
  const p = await open({
    multiple: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (p) setPath(p);
}

function setPath(p) {
  inputPath = p;
  el("path").textContent = p;
  el("go").disabled = running || !p;
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
  const files = e.dataTransfer?.files;
  if (files && files.length && files[0].name.toLowerCase().endsWith(".pdf")) {
    setPath(files[0].path || files[0].name);
  }
});

// Tauri 2 的拖拽事件带真实路径。
listen("tauri://drag-drop", (e) => {
  const payload = e.payload || {};
  const paths = payload.paths || [];
  const pdf = paths.find((p) => p.toLowerCase().endsWith(".pdf"));
  if (pdf) setPath(pdf);
});

el("go").addEventListener("click", async () => {
  if (!inputPath || running) return;
  const stem = inputPath.replace(/\.pdf$/i, "");
  let out;
  try {
    out = await save({
      defaultPath: stem + "-searchable.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
  } catch (err) {
    log("无法打开保存对话框: " + err);
    return;
  }
  if (!out) return;
  setRunning(true);
  el("done").style.display = "none";
  el("log").textContent = "";
  log("开始识别: " + inputPath);
  try {
    const s = await invoke("ocr_process", { input: inputPath, output: out, dpi: 200 });
    el("done").style.display = "block";
    el("done").innerHTML =
      `完成：<b>${s.pages_ocr}</b> 页识别加层，` +
      `${s.pages_skipped} 页原生跳过，共 ${s.lines} 行文本。<br>输出：${s.output}`;
    log("完成 ✅ 输出: " + s.output);
  } catch (err) {
    const msg = String(err);
    log(msg.includes("cancelled") ? "已取消。" : "失败：" + msg);
  }
  setRunning(false);
});

el("cancel").addEventListener("click", () => invoke("ocr_cancel"));

listen("ocr-progress", (e) => {
  const p = e.payload;
  el("status").textContent = `页 ${p.page}/${p.total} · ${p.phase}`;
  el("bar").style.width = `${((p.page / p.total) * 100).toFixed(1)}%`;
});

setRunning(false);
