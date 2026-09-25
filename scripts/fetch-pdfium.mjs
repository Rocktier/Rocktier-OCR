#!/usr/bin/env node
/**
 * Rocktier PDF Editor — fetch prebuilt Pdfium binaries.
 *
 * Downloads the platform-appropriate Pdfium dynamic library from
 * https://github.com/bblanchon/pdfium-binaries and places it in
 * src-tauri/resources/pdfium-runtime/<triple>/ so it can be bundled with the app.
 *
 * Usage:
 *   node scripts/fetch-pdfium.mjs              # current platform only
 *   node scripts/fetch-pdfium.mjs --all        # every supported platform
 *   node scripts/fetch-pdfium.mjs --force      # re-download even if present
 *   node scripts/fetch-pdfium.mjs --build=<n>  # a specific upstream build
 *
 * Offline note: this is the ONLY step that touches the network, and it runs at
 * build time — never at app runtime. Once bundled, the app is 100% offline.
 *
 * The upstream build is pinned (see `PDFIUM_BUILD`) instead of being read from
 * `releases/latest`. That endpoint is `api.github.com`, where requests without
 * an Authorization header are capped at 60/hour *per source IP* — and every
 * GitHub-hosted runner shares an egress IP with everyone else, so CI failed
 * intermittently with `HTTP 403`. The release *download* URLs live on
 * github.com rather than api.github.com and are not limited that way, so
 * building the URL ourselves removes the failure mode entirely. It also fixes
 * a second problem: upstream cuts a new build every Monday, so "latest" meant
 * the same commit could produce different binaries from one week to the next.
 */

import { execFileSync } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { copyFile, mkdir, readdir, rm, stat } from 'node:fs/promises';
import { get } from 'node:https';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');
const OUT_ROOT = join(ROOT, 'src-tauri', 'resources', 'pdfium-runtime');

/** Tauri target triples we ship for. */
const TARGETS = {
  'windows-x64': { host: 'win32', arch: 'x64', asset: 'pdfium-win-x64.tgz', file: 'pdfium.dll' },
  'macos-arm64': { host: 'darwin', arch: 'arm64', asset: 'pdfium-mac-arm64.tgz', file: 'libpdfium.dylib' },
  'macos-x64': { host: 'darwin', arch: 'x64', asset: 'pdfium-mac-x64.tgz', file: 'libpdfium.dylib' },
  'linux-x64': { host: 'linux', arch: 'x64', asset: 'pdfium-linux-x64.tgz', file: 'libpdfium.so' },
};

/**
 * The upstream Pdfium build this project ships. Upstream tags builds as
 * `chromium/<build>`, where <build> is the Chromium revision.
 *
 * Bumping this is a deliberate act rather than something CI does behind our
 * back: pick a build from
 * https://github.com/bblanchon/pdfium-binaries/releases, run this script with
 * `--build=<n> --force`, run the test suite against the result, and only then
 * change this constant.
 */
const PDFIUM_BUILD = '8057';

/**
 * Download URL for one asset of a given upstream build. No API call involved.
 *
 * Upstream does not publish checksums for these archives, so unlike
 * `provision-qpdf-windows.ps1` there is no hash to verify against. What guards
 * the contents is that extraction fails loudly on a truncated download, and
 * `findLib` fails if the archive turns out to hold no Pdfium library.
 */
function assetUrl(build, asset) {
  return `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/${build}/${asset}`;
}

function download(url, dest) {
  return new Promise((ok, fail) => {
    const req = get(url, { headers: { 'User-Agent': 'rocktier-pdf-editor' } }, (res) => {
      const { statusCode, headers } = res;
      if (statusCode >= 300 && statusCode < 400 && headers.location) {
        res.resume();
        return download(headers.location, dest).then(ok, fail);
      }
      if (statusCode !== 200) {
        res.resume();
        return fail(new Error(`HTTP ${statusCode} downloading ${url}`));
      }
      const ws = createWriteStream(dest);
      res.pipe(ws);
      ws.on('finish', () => ws.close(ok));
      ws.on('error', fail);
    });
    req.on('error', fail);
    req.setTimeout(120000, () => req.destroy(new Error('download timeout')));
  });
}

async function findLib(dir, filename) {
  const entries = await readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      const found = await findLib(p, filename);
      if (found) return found;
    } else if (e.name === filename || /^libpdfium\.(dylib|so)(\.\d+)?$/.test(e.name) || /^pdfium\.dll$/.test(e.name)) {
      return p;
    }
  }
  return null;
}

async function fetchTarget(key, build, force) {
  const t = TARGETS[key];
  // 扁平落盘：加载器（candidate_dirs + pdfium_platform_library_name_at_path）
  // 直接在 pdfium-runtime/ 下按平台名找文件，不再按 triple 分子目录。
  const destDir = OUT_ROOT;
  const destFile = join(destDir, t.file);

  if (!force) {
    try {
      const s = await stat(destFile);
      if (s.size > 100_000) return { key, skipped: true, size: s.size };
    } catch {
      /* not present, download */
    }
  }

  const tmp = join(tmpdir(), `pdfium-${key}-${Date.now()}`);
  const archive = `${tmp}.tgz`;
  await mkdir(tmp, { recursive: true });
  await download(assetUrl(build, t.asset), archive);

  // Windows 10+ and macOS both ship a BSD-compatible tar.
  execFileSync('tar', ['-xzf', archive, '-C', tmp], { stdio: 'pipe' });

  const lib = await findLib(tmp, t.file);
  if (!lib) throw new Error(`Extracted archive for ${key} contains no pdfium library`);

  await mkdir(destDir, { recursive: true });
  // copyFile (not rename): temp dir and project often live on different drives.
  await copyFile(lib, destFile);
  await rm(tmp, { recursive: true, force: true });
  await rm(archive, { force: true });

  const s = await stat(destFile);
  return { key, skipped: false, size: s.size };
}

async function main() {
  const args = process.argv.slice(2);
  const all = args.includes('--all');
  const force = args.includes('--force');
  const override = args.find((a) => a.startsWith('--build='));
  const build = override ? override.slice('--build='.length) : PDFIUM_BUILD;

  const currentKey = Object.keys(TARGETS).find(
    (k) => TARGETS[k].host === process.platform && TARGETS[k].arch === process.arch
  );
  // 位置参数可点名平台（如 windows-x64），CI 上各平台只取自己那份。
    const wanted = args.filter((a) => TARGETS[a]);
    const keys = all ? Object.keys(TARGETS) : wanted.length ? wanted : [currentKey];

  if (!currentKey && !all) {
    console.error(`Unsupported platform ${process.platform}/${process.arch}. Use --all to fetch anyway.`);
    process.exit(1);
  }

  console.log(`Fetching Pdfium build ${build} (pinned here, not "latest") ...`);

  const results = [];
  for (const key of keys) {
    if (!key) continue;
    results.push(await fetchTarget(key, build, force));
  }

  for (const r of results) {
    const mb = (r.size / 1024 / 1024).toFixed(2);
    console.log(`  ${r.skipped ? '=' : '+'} ${r.key.padEnd(12)} ${r.skipped ? 'up to date' : 'installed'} (${mb} MB)`);
  }
  console.log('Done. Pdfium binaries are in src-tauri/resources/pdfium-runtime/');
}

main().catch((e) => {
  console.error('fetch-pdfium failed:', e.message);
  process.exit(1);
});
