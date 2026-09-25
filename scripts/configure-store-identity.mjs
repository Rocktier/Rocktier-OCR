#!/usr/bin/env node
/**
 * 把 Partner Center 分配的身份写进 tauri-windows-bundle 的配置。
 *
 * 在 `npx tauri-windows-bundle init` 之后、`build` 之前跑：
 *   node scripts/configure-store-identity.mjs [identityName]
 *
 * - publisher / publisherDisplayName 是**账号级**的，与 PDF 产品相同，
 *   直接沿用家族值（除非账号换了）。
 * - identityName 是 Partner Center 为这个产品分配的 Package/Identity Name
 *   （形如 12345Rocktier.RocktierOCR）。缺省时写占位符，只够测试构建；
 *   上架前必须换成真值。
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

const identityName = process.argv[2] || 'PLACEHOLDER.Set.From.PartnerCenter';

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
if (identityName.startsWith('PLACEHOLDER')) {
  console.log('  ⚠️ 占位身份：这个 msix 只能本机侧载测试，不能提交商店。');
}
