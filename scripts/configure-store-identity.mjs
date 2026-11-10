#!/usr/bin/env node
/**
 * 把 Partner Center 分配的身份写进 tauri-windows-bundle 的配置。
 *
 * 在 `npx tauri-windows-bundle init` 之后、`build` 之前跑：
 *   node scripts/configure-store-identity.mjs [identityName]
 *
 * - publisher / publisherDisplayName 是**账号级**的，与 PDF 产品相同，
 *   直接沿用家族值（除非账号换了）。
 * - identityName 是 Partner Center 为这个产品分配的 Package/Identity Name。
 *   OCR 的真值是 Rocktier.RocktierOCR（2026-09-25 用户提供，与
 *   src-tauri/tauri.conf.json 的 identifier 一致），**不是占位符**。
 *   脚本会硬断言：占位名 / 非法格式 / 与 identifier 不一致一律 exit 1，
 *   避免「占位身份静默出包、提交时才被拒」（Pic2WebP 2026-09-17 实测
 *   com.rocktier.pic2webp 被上传校验直接拒绝）。
 * - identityName 存在的话，同时补写生成物里的 AppxManifest.xml（init 生成
 *   的清单里 Identity Name 来自 productName，与商店分配值不一致时以它为准）。
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { readdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');
const GEN = join(ROOT, 'src-tauri', 'gen', 'windows');

// 家族账号级值（与 Rocktier PDF 的 gen/windows/bundle.config.json 相同）。
const PUBLISHER = 'CN=4EA39D7A-401B-4D56-98D0-8ECB1F2B8DF7';
const PUBLISHER_DISPLAY = 'Rocktier';
const EXECUTABLE = 'RocktierOCR';

// Partner Center 为本产品分配的 Package/Identity Name（用户提供，2026-09-25）。
// 真值常量而非占位：缺省就用它，显式传参只用于临时验证，且必须同值。
const STORE_IDENTITY_NAME = 'Rocktier.RocktierOCR';
const identityName = process.argv[2] || STORE_IDENTITY_NAME;

// 判据 1：占位名 / 非法格式一律拒绝，不让商店包带着假身份出门。
if (identityName.startsWith('PLACEHOLDER') ||
    !/^[A-Za-z0-9]+\.[A-Za-z0-9.]+$/.test(identityName)) {
  console.error(`  x 非法或占位的 Package Identity Name: ${identityName}`);
  process.exit(1);
}
// 判据 2：与 tauri.conf.json 的 identifier 逐字一致（防家族改名后漂移）。
const tauriConf = JSON.parse(
  readFileSync(join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf8')
);
if (tauriConf.identifier !== identityName) {
  console.error(`  x Identity Name 与 tauri.conf.json identifier 不一致: ` +
                `${identityName} vs ${tauriConf.identifier}`);
  process.exit(1);
}
console.log(`  ok Identity Name 校验通过: ${identityName}`);

const configPath = join(GEN, 'bundle.config.json');
const config = JSON.parse(readFileSync(configPath, 'utf8'));
config.publisher = PUBLISHER;
config.publisherDisplayName = PUBLISHER_DISPLAY;
config.executableName = EXECUTABLE;
// OCR 完全离线：不要任何 capability（runFullTrust 工具会自动加）。
config.capabilities = { general: [] };
writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
console.log(`  bundle.config.json: publisher=${PUBLISHER} executable=${EXECUTABLE}`);

// 身份名：生成物里凡是有 Identity Name= 的清单都改成商店分配值。
function walk(dir) {
  let out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out = out.concat(walk(p));
    else if (e.name === 'AppxManifest.xml') out.push(p);
  }
  return out;
}
let patched = 0;
for (const f of walk(GEN)) {
  const s = readFileSync(f, 'utf8');
  if (!s.includes('<Identity ')) continue;
  const next = s.replace(/(<Identity [^>]*Name=")[^"]*(")/, `$1${identityName}$2`);
  if (next !== s) {
    writeFileSync(f, next);
    patched++;
  }
}
console.log(`  AppxManifest: ${patched} 个清单的 Identity Name → ${identityName}`);
