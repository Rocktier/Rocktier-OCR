#!/usr/bin/env node
// 校验 MSIX 宽磁贴是真实图案，而不是 tauri-windows-bundle 生成的纯色占位图。
//
// 背景：@choochmeque/tauri-windows-bundle 的 TAURI_ICON_MAP 只映射
// StoreLogo / Square44x44Logo / Square150x150Logo，不含 Wide310x150Logo。
// `init --scale` 每次都会为宽磁贴另造一张图；OCR 0.2.13.0 首包里那张是
// 310x150 的纯黑 RGBA 图（554 bytes），肉眼几乎看不出但商店会拒。
//
// 用法：node scripts/verify-wide-tile.mjs <png>
// 退出码：0 = 真图，1 = 判红（CI 会因此失败，不会静默出坏包）。

import { readFileSync } from 'node:fs';
import { inflateSync } from 'node:zlib';

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function fail(message) {
  console.error(`FAIL: ${message}`);
  process.exit(1);
}

const target = process.argv[2];
if (!target) fail('usage: node verify-wide-tile.mjs <png>');

let buf;
try {
  buf = readFileSync(target);
} catch (error) {
  fail(`cannot read ${target}: ${error.message}`);
}

if (buf.length < 8 || !buf.subarray(0, 8).equals(PNG_SIGNATURE)) {
  fail('not a PNG file');
}

// 纯色图经 deflate 后极小；真图带细节，体积不可能只有几百字节。
// 这一条只是快速拦截，真正判据是下面的颜色数。
if (buf.length < 4000) {
  fail(`only ${buf.length} bytes - looks like a generated solid placeholder`);
}

let offset = 8;
let ihdr = null;
const idat = [];
while (offset + 8 <= buf.length) {
  const length = buf.readUInt32BE(offset);
  const type = buf.toString('ascii', offset + 4, offset + 8);
  const data = buf.subarray(offset + 8, offset + 8 + length);
  if (type === 'IHDR') ihdr = data;
  else if (type === 'IDAT') idat.push(data);
  else if (type === 'IEND') break;
  offset += 12 + length;
}
if (!ihdr || ihdr.length < 13) fail('missing IHDR');
if (idat.length === 0) fail('missing IDAT');

const width = ihdr.readUInt32BE(0);
const height = ihdr.readUInt32BE(4);
const bitDepth = ihdr[8];
const colorType = ihdr[9];
const interlace = ihdr[12];

if (width !== 310 || height !== 150) fail(`wrong size ${width}x${height}, expected 310x150`);
if (bitDepth !== 8 || (colorType !== 2 && colorType !== 6)) {
  fail(`unsupported PNG format: bitDepth=${bitDepth} colorType=${colorType}`);
}
if (interlace !== 0) fail('interlaced PNG is not supported by this check');

const channels = colorType === 2 ? 3 : 4;
const stride = width * channels;
let raw;
try {
  raw = inflateSync(Buffer.concat(idat));
} catch (error) {
  fail(`cannot inflate IDAT: ${error.message}`);
}
if (raw.length !== (stride + 1) * height) {
  fail(`unexpected scanline length: ${raw.length}, expected ${(stride + 1) * height}`);
}

// 还原 PNG 逐行 filter，统计不同像素颜色。纯色占位 = 1 种颜色。
const pixels = Buffer.alloc(stride * height);
let pos = 0;
for (let y = 0; y < height; y += 1) {
  const filter = raw[pos];
  pos += 1;
  const row = raw.subarray(pos, pos + stride);
  pos += stride;
  for (let x = 0; x < stride; x += 1) {
    const left = x >= channels ? pixels[y * stride + x - channels] : 0;
    const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
    const upLeft = x >= channels && y > 0 ? pixels[(y - 1) * stride + x - channels] : 0;
    let value = row[x];
    if (filter === 1) value = (value + left) & 255;
    else if (filter === 2) value = (value + up) & 255;
    else if (filter === 3) value = (value + ((left + up) >> 1)) & 255;
    else if (filter === 4) {
      const p = left + up - upLeft;
      const pa = Math.abs(p - left);
      const pb = Math.abs(p - up);
      const pc = Math.abs(p - upLeft);
      const predictor = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
      value = (value + predictor) & 255;
    } else if (filter !== 0) {
      fail(`invalid PNG filter type ${filter}`);
    }
    pixels[y * stride + x] = value;
  }
}

const colors = new Set();
for (let i = 0; i < pixels.length; i += channels) {
  colors.add(`${pixels[i]},${pixels[i + 1]},${pixels[i + 2]}`);
}
if (colors.size < 8) {
  fail(`only ${colors.size} distinct RGB color(s) - solid placeholder, not a real tile`);
}

console.log(
  `PASS: ${target} - 310x150, colorType=${colorType}, ` +
    `${colors.size} distinct colors, ${buf.length} bytes`,
);
