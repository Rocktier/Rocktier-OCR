/* 更新检查 —— 只在用户主动要求时才联网。
 *
 * ── 为什么不用 Tauri 的 updater 插件 ──────────────────────────────
 *
 * 家族对外的承诺是「除非你要求，应用不会联网」，且「绝不在未经询问的
 * 情况下下载或安装任何东西」（privacy.html 的 Update checks 一节）。
 *
 * Tauri updater 插件会在应用里常驻一个「下载并替换自身」的通道，需要
 * 为每款产品维护签名密钥对与 CI secret。而我们并不需要它：用户想要的是
 * 「有没有新版本，去哪下」，不是「静默替我装好」。
 *
 * 所以这里只做一件事：请求一个列出最新版本的公开文件，比对版本号，
 * 把结果和官方发布页链接交给用户。下载与安装由用户自己决定。
 * 全程零 Rust 改动、零密钥、零新增依赖。
 *
 * ── 请求里有什么 ──────────────────────────────────────────────────
 *
 * 只有 GitHub Releases 的公开 API 地址。不含许可证码、设备标识、
 * 机器信息，或与文件有关的任何内容。这也是它能被写进隐私政策的理由。
 *
 * ── OCR 特有 ──────────────────────────────────────────────────────
 *
 * 原生 JS 无打包器：版本号取 window.__TAURI__.app.getVersion()（与
 * tauri.conf.json 同源，零漂移）；文案走 lang.js 的 tr().update.*；
 * 语言切换沿家族 applyLang 包装链再包一层，切语言时重渲染角落。
 */
(function () {
  "use strict";

  var REPO = "Rocktier/Rocktier-OCR";
  var API = "https://api.github.com/repos/" + REPO + "/releases/latest";
  var FALLBACK_URL = "https://github.com/" + REPO + "/releases/latest";
  var STORE_KEY = "rocktier.updateCheckedAt";

  var state = "idle"; // idle | checking | uptodate | available | failed
  var result = null;
  var lastCheckedAt = loadLastChecked();
  var checking = false;
  var corner = null;
  var APP_VERSION = "";

  function tr() {
    return typeof window.tr === "function" ? window.tr() : {};
  }
  function label(key, fallback) {
    var u = tr().update || {};
    return u[key] || fallback;
  }

  function parse(v) {
    return String(v)
      .replace(/^v/i, "")
      .split(".")
      .map(function (p) { return parseInt(p, 10) || 0; });
  }
  function isNewer(a, b) {
    var pa = parse(a);
    var pb = parse(b);
    for (var i = 0; i < Math.max(pa.length, pb.length); i += 1) {
      var x = pa[i] || 0;
      var y = pb[i] || 0;
      if (x > y) return true;
      if (x < y) return false;
    }
    return false;
  }

  function appVersion() {
    try {
      return window.__TAURI__.app
        .getVersion()
        .then(function (v) { APP_VERSION = v || ""; render(); })
        .catch(function () {});
    } catch (e) {
      return Promise.resolve();
    }
  }

  function loadLastChecked() {
    try {
      var n = Number(localStorage.getItem(STORE_KEY));
      return n > 0 ? n : null;
    } catch (e) {
      return null;
    }
  }

  function formatCheckedAt(ts) {
    try {
      var loc = (document.documentElement.lang || "en").indexOf("zh") === 0 ? "zh-CN" : "en";
      return new Date(ts).toLocaleDateString(loc);
    } catch (e) {
      return new Date(ts).toISOString().slice(0, 10);
    }
  }

  async function checkForUpdates() {
    var res = await fetch(API, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) throw new Error("HTTP " + res.status);
    var data = await res.json();
    var latest = String(data.tag_name || "").replace(/^v/i, "");
    if (!latest) throw new Error("release has no tag_name");
    return {
      latest: latest,
      url: data.html_url || FALLBACK_URL,
      hasUpdate: isNewer(latest, APP_VERSION),
      checkedAt: Date.now(),
    };
  }

  async function runUpdateCheck() {
    if (checking) return;
    checking = true;
    state = "checking";
    render();
    try {
      var r = await checkForUpdates();
      result = r;
      lastCheckedAt = r.checkedAt;
      try { localStorage.setItem(STORE_KEY, String(r.checkedAt)); } catch (e) { /* 隐私模式下忽略 */ }
      state = r.hasUpdate ? "available" : "uptodate";
      /* 顺带吊销复检：同一次点击、同一承诺（只在点击时联网）。revoked → 清回执，
         授权状态回到未激活并弹激活对话框（license.js 全局函数，先于本文件加载）。 */
      var rv = await revalidateLicence();
      if (rv === "revoked") {
        refreshLicenseStatus();
        openLicenseDialog();
      }
    } catch (e) {
      state = "failed";
    } finally {
      checking = false;
      render();
    }
  }

  function ensureCorner() {
    if (corner) return;
    corner = document.createElement("div");
    corner.className = "update-corner";
    corner.innerHTML =
      '<button type="button" class="update-line">' +
      '<span class="update-ver"></span>' +
      '<span class="update-sep" aria-hidden="true">·</span>' +
      '<span class="update-action"></span>' +
      "</button>" +
      '<div class="update-note" role="status" aria-live="polite"></div>';
    document.body.appendChild(corner);
    corner.querySelector(".update-line").addEventListener("click", function () {
      void runUpdateCheck();
    });
    render();
  }

  function render() {
    if (!corner) return;
    var u = tr().update || {};
    corner.classList.toggle("has-update", state === "available");
    corner.querySelector(".update-ver").textContent = "v" + (APP_VERSION || "?");
    corner.querySelector(".update-line").disabled = state === "checking";
    corner.querySelector(".update-line").title = label("privacy", "Only checks when you click");
    corner.querySelector(".update-action").textContent =
      state === "checking" ? label("checking", "Checking…") : label("check", "Check for updates");
    var note = corner.querySelector(".update-note");
    note.textContent = "";
    if (state === "uptodate") {
      note.textContent = label("upToDate", "You're up to date");
    } else if (state === "failed") {
      note.textContent = label("failed", "Couldn't check for updates");
    } else if (state === "available" && result) {
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "update-get";
      btn.textContent = label("get", "Get ") + result.latest;
      btn.title = label("avail", "New version: ") + result.latest;
      btn.addEventListener("click", function () {
        try {
          window.__TAURI__.core
            .invoke("open_url", { url: result.url })
            .catch(function () {});
        } catch (e) {
          window.open(result.url, "_blank", "noopener");
        }
      });
      note.appendChild(btn);
    } else {
      note.textContent =
        label("lastPrefix", "Last checked: ") +
        (lastCheckedAt ? formatCheckedAt(lastCheckedAt) : label("never", "Never"));
    }
  }

  // 语言切换链（lang.js → app.js → license.js → 本层）：重渲染角落文案
  var origApplyLang = window.applyLang;
  window.applyLang = function () {
    if (typeof origApplyLang === "function") origApplyLang();
    render();
  };

  // 菜单入口（Help → 检查更新）：app.js 的 menu-action 分支会调它
  window.ocrRunUpdateCheck = function () {
    ensureCorner();
    void runUpdateCheck();
  };

  ensureCorner();
  appVersion();
})();
