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

  var state = "idle"; // idle | checking | uptodate | available | failed
  var result = null;
  var checking = false;
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
    openUpdateDialog();
    try {
      var r = await checkForUpdates();
      result = r;
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
      renderDialog();
    }
  }

  /* ── 对话框（结构照 license.js 的 overlay 模式）──────────────────────────
     入口只在帮助菜单「检查更新…」—— 点击才联网，启动时、工作中、后台都
     不检查。结果一律在这里展示：右下角常驻悬浮入口已移除（用户要求入口
     全部收进菜单；fixed 定位实测还会盖住底部内容，Compressor v0.1.10），
     统一改为菜单 + 弹框。 */
  var overlay = null;

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function buildDialog() {
    overlay = el("div", "update-overlay");
    overlay.hidden = true;

    var dialog = el("div", "update-dialog");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");

    var header = el("div", "update-header");
    var title = el("h3", null, label("check", "Check for updates"));
    header.appendChild(title);
    var closeX = el("button", "update-close", "\u00d7");
    closeX.type = "button";
    closeX.setAttribute("aria-label", label("close", "Close"));
    closeX.addEventListener("click", closeUpdateDialog);
    header.appendChild(closeX);

    var body = el("div", "update-body");
    var footer = el("div", "update-footer");

    dialog.appendChild(header);
    dialog.appendChild(body);
    dialog.appendChild(footer);
    overlay.appendChild(dialog);
    // 点遮罩 = 关闭；面板内点击不冒泡（与 license.js 一致）。
    overlay.addEventListener("mousedown", function (e) {
      if (e.target === overlay) closeUpdateDialog();
    });
    // Esc 关闭（对话框打开期间生效）。
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && !overlay.hidden) closeUpdateDialog();
    });
    document.body.appendChild(overlay);
  }

  function openUpdateDialog() {
    if (!overlay) buildDialog();
    overlay.hidden = false;
    renderDialog();
  }

  function closeUpdateDialog() {
    if (overlay) overlay.hidden = true;
  }

  function openUrl(url) {
    try {
      window.__TAURI__.core.invoke("open_url", { url: url }).catch(function () {});
    } catch (e) {
      window.open(url, "_blank", "noopener");
    }
  }

  function line(text, small) {
    var p = el("p", null, text);
    if (small) {
      p.appendChild(document.createElement("br"));
      var s = el("small", null, small);
      p.appendChild(s);
    }
    return p;
  }

  function renderDialog() {
    if (!overlay || overlay.hidden) return;
    var body = overlay.querySelector(".update-body");
    var footer = overlay.querySelector(".update-footer");
    overlay.querySelector("h3").textContent = label("check", "Check for updates");
    body.textContent = "";
    footer.textContent = "";

    if (state === "checking") {
      body.appendChild(line(label("checking", "Checking\u2026"), label("privacy", "Only checks when you click")));
    } else if (state === "uptodate") {
      body.appendChild(line(label("upToDate", "You're up to date"), "v" + (APP_VERSION || "?")));
    } else if (state === "failed") {
      body.appendChild(line(label("failed", "Couldn't check for updates"), label("openReleasesHint", "You can also open the releases page directly.")));
    } else if (state === "available" && result) {
      body.appendChild(
        line(
          label("avail", "New version: ") + "v" + result.latest,
          "v" + (APP_VERSION || "?") + " \u2192 v" + result.latest
        )
      );
    }

    var closeBtn = el("button", null, label("close", "Close"));
    closeBtn.type = "button";
    closeBtn.addEventListener("click", closeUpdateDialog);
    footer.appendChild(closeBtn);

    var spacer = el("span", "update-footer-spacer");
    footer.appendChild(spacer);

    if (state === "failed") {
      var releasesBtn = el("button", null, label("openReleases", "Open releases"));
      releasesBtn.type = "button";
      releasesBtn.addEventListener("click", function () {
        openUrl("https://github.com/" + REPO + "/releases/latest");
      });
      footer.appendChild(releasesBtn);
    }
    if (state === "available" && result) {
      var getBtn = el("button", "update-get-btn", label("get", "Get ") + " v" + result.latest);
      getBtn.type = "button";
      getBtn.addEventListener("click", function () {
        openUrl(result.url);
      });
      footer.appendChild(getBtn);
    }
  }

  // 语言切换链（lang.js → app.js → license.js → 本层）：重渲染对话框文案
  var origApplyLang = window.applyLang;
  window.applyLang = function () {
    if (typeof origApplyLang === "function") origApplyLang();
    renderDialog();
  };

  // 菜单入口（Help → 检查更新）：app.js 的 menu-action 分支会调它
  window.ocrRunUpdateCheck = function () {
    openUpdateDialog();
    void runUpdateCheck();
  };

  appVersion();
})();
