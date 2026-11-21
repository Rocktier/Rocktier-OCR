#!/usr/bin/env node
/* Rocktier OCR · MAS 沙盒授权静态判据
 *
 * 用法:
 *   node scripts/verify-mas-entitlements.mjs             主检查（真实仓）
 *   node scripts/verify-mas-entitlements.mjs --selftest  判据自身的正反对照
 *
 * 为什么需要它：
 *   Tauri 前端资源走 http://asset.localhost / http://ipc.localhost 回环，
 *   沙盒下缺 com.apple.security.network.client 时内核拒 socket，Webview
 *   加载不到 index.html，审核员看到的就是白屏 —— 2026-11-20 OCR 1.0.0(5)
 *   被拒原文「app launched to a blank screen」。这条授权在注释里被写成
 *   「刻意不开」长达数月，代码分析与 codesign --verify 都查不出来，
 *   只有真机启动或本判据能抓。
 *
 * 证据边界：静态判据只证明 plist 写对了；真机 A/B 见 findings D-168。
 */

import { readFileSync, existsSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const REL = "src-tauri/Entitlements.appstore.plist";

function read(root) {
  const p = join(root, REL);
  return existsSync(p) ? readFileSync(p, "utf8").replace(/\r\n/g, "\n") : "";
}

function keyOn(text, key) {
  /* 只认真正的 <key>…</key><true/> 配对，注释里提到 key 名不算。 */
  const re = new RegExp(
    `<key>${key.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&")}</key>\\s*<true\\s*/>`,
    "m",
  );
  return re.test(text);
}

function check(root) {
  const text = read(root);
  const out = [];
  const add = (id, ok, detail) => out.push({ id, ok, detail });

  add("plist", !!text, `${REL} 存在`);
  add("sandbox", keyOn(text, "com.apple.security.app-sandbox"), "MAS 沙盒总开关");
  add(
    "files",
    keyOn(text, "com.apple.security.files.user-selected.read-write"),
    "用户选中文件读写",
  );
  add(
    "loopback",
    keyOn(text, "com.apple.security.network.client"),
    "Webview 加载自身前端资源所需的 loopback 通道（缺此项 = 白屏）",
  );

  return out;
}

function selftest() {
    const tmp = mkdtempSync(join(tmpdir(), "ocr-mas-ent-"));
    mkdirSync(join(tmp, "src-tauri"), { recursive: true });
    try {
    const good = read(ROOT);
    if (!good) throw new Error(`真实仓里没有 ${REL}，无法做正反对照`);
    writeFileSync(join(tmp, REL), good, "utf8");

    const base = check(tmp);
    const bad = base.map((x) => (x.id === "loopback" ? { ...x, ok: false } : x));

    /* 正例：注释提及 key 名但没写 <key> 时也必须判红。 */
    const commentOnly = good.replace(
      /<key>com\.apple\.security\.network\.client<\/key>\s*<true\/>/,
      "<!-- com.apple.security.network.client 只在注释里 -->",
    );
    writeFileSync(join(tmp, REL), commentOnly, "utf8");
    const commentResult = check(tmp).find((x) => x.id === "loopback");

    const cases = [
      ["修复后全绿", base.every((x) => x.ok), true],
      ["去掉 loopback 必红", bad.find((x) => x.id === "loopback").ok, false],
      ["只写注释不算数", commentResult.ok, false],
    ];
    let failed = 0;
    for (const [name, actual, expected] of cases) {
      const ok = actual === expected;
      if (!ok) failed++;
      console.log(`${ok ? "PASS" : "FAIL"}  ${name}: actual=${actual} expected=${expected}`);
    }
    return failed;
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

const isSelf = process.argv.includes("--selftest");
const results = isSelf ? null : check(ROOT);
if (isSelf) {
  process.exitCode = selftest() ? 1 : 0;
} else {
  let failed = 0;
  for (const x of results) {
    console.log(`${x.ok ? "PASS" : "FAIL"}  ${x.id}: ${x.detail}`);
    if (!x.ok) failed++;
  }
  if (failed) {
    console.error(`\n${failed} 项判据失败 —— MAS 包会白屏，禁止出包。`);
    process.exitCode = 1;
  }
}
