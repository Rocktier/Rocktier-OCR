// ─── License: trial & activation（家族 L6，Rust 侧见 src-tauri/src/license.rs）──
// Vanilla 实现，照 Pic2WebP 提交 4f669ac 的 src/license.js 移植；OCR 没有 UI 框架、
// 也没有打包器，脚本走经典 <script>（window.__TAURI__ 全局，与 lang.js / app.js 一致），
// 对话框用 document.createElement 就地构建。激活协议照 PDF / MD：前端只做一次
// fetch 换回执，验签与落盘在 Rust（store_receipt）。

// 产品页：购买与试用说明的唯一入口（与 menu.rs 的 open_url 白名单同源前缀）。
const LICENSE_BUY_URL = "https://rocktier.com/ocr";

// 最近一次 license_status 的结果（LicenseInfo，见 main.rs）。
let licenseInfo = null;
// 对话框元素引用（首次打开时构建，之后复用）。
let licenseEls = null;

// 写命令被授权闸门拦下的统一判据：Rust 的 ensure_write_allowed 返回的错误码固定为
// LICENSE_EXPIRED。各调用点的 catch 用它分流"弹激活对话框"还是"普通失败提示"。
function isLicenseExpiredError(e) {
  return String(e).includes("LICENSE_EXPIRED");
}

function isLicenseDialogOpen() {
  return !!(licenseEls && !licenseEls.overlay.hidden);
}

// 拉取并刷新授权状态，更新头部胶囊。失败不打扰用户（dev/浏览器里没有该命令）。
async function refreshLicenseStatus() {
  if (!window.__TAURI__) return null;
  try {
    licenseInfo = await window.__TAURI__.core.invoke("license_status");
  } catch (e) {
    console.warn("license_status failed:", e);
    return null;
  }
  updatePill();
  return licenseInfo;
}

// ── 头部授权胶囊：试用剩 N 天 / 未激活；已激活或商店版下隐藏 ──

function updatePill() {
  const pill = document.getElementById("license-pill");
  if (!pill) return;
  const show = licenseInfo && licenseInfo.channel === "direct" && licenseInfo.status !== "licensed";
  pill.hidden = !show;
  if (!show) return;
  const L = tr().license;
  if (licenseInfo.status === "expired") {
    pill.textContent = L.expiredChip;
    pill.classList.add("expired");
  } else {
    pill.textContent = L.trialChip(licenseInfo.daysLeft);
    pill.classList.remove("expired");
  }
}

// ── 激活：一次联网换回执，之后离线验签、永不联网 ──

async function activateLicense(code) {
  let payload;
  try {
    /* 上报机器指纹 —— 服务端据此限制「一张码能激活几台设备」。
       取不到时是空串，服务端不计数也不拦激活（见 api/devices.js）。
       指纹只用于设备计数，不含任何硬件序列号原文。 */
    let fingerprint = "";
    try {
      fingerprint = await window.__TAURI__.core.invoke("report_machine_fingerprint");
    } catch {
      // Rust 命令不可用（极旧版本）不该阻断激活。
    }
    const res = await fetch("https://rocktier.com/api/activate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: code.trim(), fingerprint, os: (window.navigator && window.navigator.platform) || "" }),
    });
    payload = await res.json();
    if (!res.ok || !payload.receipt) {
      throw new Error(payload.error || `activation failed (${res.status})`);
    }
  } catch (e) {
    // 断网是最常见情形 —— 明说，而不是甩一条 fetch 报错。
    const msg = e instanceof Error && e.message && !e.message.includes("fetch")
      ? e.message
      : "offline";
    throw new Error(msg);
  }
  // 验签与落盘在 Rust：前端拿到的只是一段待验的字符串。
  return window.__TAURI__.core.invoke("store_receipt", { signed: payload.receipt });
}

// ── 对话框（createElement 实现：遮罩 + 面板 + 输入框 + 按钮）──

function buyLicense() {
  // open_url 的白名单放行 https://rocktier.com/ 前缀（menu.rs）。
  if (window.__TAURI__) {
    window.__TAURI__.core.invoke("open_url", { url: LICENSE_BUY_URL }).catch(() => {});
  } else {
    window.open(LICENSE_BUY_URL, "_blank", "noopener");
  }
}

// 三种失败要分开说，用户的下一步动作不同：没连上网（重试即可）、码属于别的
// 应用（要买对单品或全家桶）、码不对（检查有没有抄错）。
function licenseErrorKey(detail) {
  if (detail === "offline") return "offline";
  if (detail.includes("WRONG_PRODUCT")) return "wrongProduct";
  if (detail.includes("REFUNDED")) return "refunded";
  return "invalid";
}

function buildLicenseDialog() {
  const overlay = document.createElement("div");
  overlay.className = "license-overlay";
  overlay.hidden = true;

  const dialog = document.createElement("div");
  dialog.className = "license-dialog";
  dialog.setAttribute("role", "dialog");
  dialog.setAttribute("aria-modal", "true");

  const header = document.createElement("div");
  header.className = "license-header";
  const title = document.createElement("h3");
  header.appendChild(title);
  const closeX = document.createElement("button");
  closeX.type = "button";
  closeX.className = "license-close";
  closeX.textContent = "×";
  closeX.addEventListener("click", closeLicenseDialog);
  header.appendChild(closeX);

  const body = document.createElement("div");
  body.className = "license-body";

  const footer = document.createElement("div");
  footer.className = "license-footer";

  dialog.append(header, body, footer);
  overlay.appendChild(dialog);
  // 点遮罩 = 取消；面板内点击不冒泡。
  overlay.addEventListener("mousedown", (e) => {
    if (e.target === overlay) closeLicenseDialog();
  });
  // Esc 关闭（对话框打开期间生效）。
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !overlay.hidden) closeLicenseDialog();
  });
  document.body.appendChild(overlay);

  licenseEls = { overlay, title, body, footer, closeX };
}

function makeLicenseButton(className, text, onClick) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = className;
  btn.textContent = text;
  btn.addEventListener("click", onClick);
  return btn;
}

// 每次打开/状态变化都整块重建正文：文案走 tr().license，语言切换后也总是当前语言。
function renderLicenseDialog() {
  if (!licenseEls) buildLicenseDialog();
  const L = tr().license;
  const { title, body, footer } = licenseEls;
  // 语言切换重建时保住已输入的激活码
  const prevInput = body.querySelector("#license-code");
  const savedCode = prevInput ? prevInput.value : "";

  title.textContent = L.title;
  licenseEls.closeX.setAttribute("aria-label", L.close);
  body.textContent = "";
  footer.textContent = "";

  const status = document.createElement("p");

  if (!licenseInfo) {
    status.textContent = L.loading;
    body.appendChild(status);
    footer.appendChild(makeLicenseButton("", L.close, closeLicenseDialog));
    return;
  }

  const licensed = licenseInfo.status === "licensed";
  const isStore = licenseInfo.channel === "store";
  // 没有公钥就没人激活得了。如实说明，而不是让付过钱的用户看到"激活码未被接受"。
  const canActivate = licenseInfo.activationConfigured !== false;
  const statusLine = () =>
    licenseInfo.status === "expired" ? L.expired : L.trialLeft(licenseInfo.daysLeft);

  if (licensed) {
    status.textContent = licenseInfo.product === "FL" ? L.licensedFamily : L.licensed;
    body.appendChild(status);
    const note = document.createElement("small");
    note.textContent = L.licensedNote;
    body.appendChild(note);
  } else if (!canActivate) {
    status.textContent = statusLine();
    body.appendChild(status);
    const note = document.createElement("small");
    note.textContent = L.notConfigured;
    body.appendChild(note);
  } else if (isStore) {
    status.textContent = statusLine();
    body.appendChild(status);
    // 商店版：说明授权由商店负责，不提供任何站外购买入口（微软政策 10.8.2/10.8.4）。
    const note = document.createElement("small");
    note.textContent = L.storeNote;
    body.appendChild(note);
  } else {
    status.textContent = statusLine();
    body.appendChild(status);

    const field = document.createElement("div");
    field.className = "license-field";
    const label = document.createElement("label");
    label.htmlFor = "license-code";
    label.textContent = L.codeLabel;
    const input = document.createElement("input");
    input.id = "license-code";
    input.type = "text";
    input.spellcheck = false;
    input.autocomplete = "off";
    input.placeholder = L.codePlaceholder;
    input.value = savedCode;
    field.append(label, input);
    body.appendChild(field);

    const where = document.createElement("small");
    where.textContent = L.whereToFind;
    body.appendChild(where);

    const privacy = document.createElement("small");
    privacy.textContent = L.privacyNote;
    body.appendChild(privacy);

    const activateBtn = makeLicenseButton("primary", L.activate, submitActivation);
    activateBtn.disabled = !savedCode.trim();
    const syncActivate = () => { activateBtn.disabled = !input.value.trim(); };
    input.addEventListener("input", syncActivate);
    // 输入框里 Enter = 激活
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") submitActivation();
    });
    footer.appendChild(makeLicenseButton("", L.close, closeLicenseDialog));
    footer.appendChild(makeLicenseButton("", L.buy, buyLicense));
    footer.appendChild(activateBtn);
    // 打开即聚焦输入框
    setTimeout(() => input.focus(), 0);
  }

  // 其余分支（loading/已激活/未配钥/商店版）只需要一个关闭按钮
  if (!footer.querySelector("button")) {
    footer.appendChild(makeLicenseButton("", L.close, closeLicenseDialog));
  }
}

let submittingActivation = false;

async function submitActivation() {
  if (!licenseEls || submittingActivation) return;
  const input = licenseEls.body.querySelector("#license-code");
  const activateBtn = licenseEls.footer.querySelector(".primary");
  const code = input ? input.value.trim() : "";
  if (!code) return;

  submittingActivation = true;
  const L = tr().license;
  if (activateBtn) {
    activateBtn.disabled = true;
    activateBtn.textContent = L.activating;
  }
  // 旧错误清掉再试
  licenseEls.body.querySelectorAll(".license-error").forEach((n) => n.remove());
  try {
    licenseInfo = await activateLicense(code);
    updatePill();
    // 激活成功立刻反映为已激活，无需重启即可继续导出
    renderLicenseDialog();
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e);
    const err = document.createElement("p");
    err.className = "license-error";
    err.textContent = tr().license[licenseErrorKey(detail)];
    licenseEls.body.appendChild(err);
    if (activateBtn) {
      activateBtn.textContent = tr().license.activate;
      activateBtn.disabled = !(input && input.value.trim());
    }
  } finally {
    submittingActivation = false;
  }
}

function openLicenseDialog() {
  if (!licenseEls) buildLicenseDialog();
  // 先用现有状态渲染（首次为检查中）
  renderLicenseDialog();
  licenseEls.overlay.hidden = false;
  // 再拉最新状态：试用可能刚好在今天到期；到达后若对话框还开着就重绘。
  refreshLicenseStatus().then(() => {
    if (isLicenseDialogOpen()) renderLicenseDialog();
  });
}

function closeLicenseDialog() {
  if (licenseEls) licenseEls.overlay.hidden = true;
}

// 识别完成后的导出提示（ocr_process 成功后由 app.js 调用）：
// 试用内提示一次「导出 PDF/TXT 需授权（试用剩 N 天）」，过期改为「需先激活」，
// 已激活或商店版（不拦截）不提示。先刷新一次状态，避免拿旧天数说话。
async function licenseExportHint() {
  await refreshLicenseStatus();
  if (!licenseInfo || !licenseInfo.enforcing) return "";
  if (licenseInfo.status === "trial") return tr().license.exportHintTrial(licenseInfo.daysLeft);
  if (licenseInfo.status === "expired") return tr().license.exportHintExpired;
  return "";
}

// ── 接线：胶囊点击、语言切换重绘、license-expired 事件 ──

(function initLicense() {
  const pill = document.getElementById("license-pill");
  if (pill) pill.addEventListener("click", openLicenseDialog);

  // OCR 的语言切换走 window.applyLang 链（lang.js 定义，app.js 与 index.html
  // 的主题脚本各包了一层）：胶囊与打开中的对话框跟着换语言。
  const origApplyLang = window.applyLang;
  window.applyLang = function () {
    if (typeof origApplyLang === "function") origApplyLang();
    updatePill();
    if (isLicenseDialogOpen()) renderLicenseDialog();
  };

  if (!window.__TAURI__) return;
  window.__TAURI__.event
    .listen("license-expired", () => openLicenseDialog())
    .catch(() => {});
  refreshLicenseStatus();
})();
