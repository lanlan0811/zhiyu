// Generates the application icons for the Tauri shell (src-tauri/icons) with
// zero external dependencies: plain Node + the built-in zlib.
//
// Outputs:
//   src-tauri/icons/32x32.png        tray / small windows
//   src-tauri/icons/128x128.png      general
//   src-tauri/icons/128x128@2x.png   hi-dpi (256x256)
//   src-tauri/icons/icon.png         512x512
//   src-tauri/icons/icon.ico         windows exe icon (png-compressed ico)
//   src-tauri/icons/icon.icns        macOS app icon (png chunks)
//
// Usage: node scripts/gen-icons.mjs

import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const outDir = join(root, 'src-tauri', 'icons');

// ---- pixel drawing ---------------------------------------------------------

// Signed distance to a rounded square centered at (cx, cy) with half-size h.
function roundedSquare(x, y, cx, cy, h, r) {
  const dx = Math.abs(x - cx) - (h - r);
  const dy = Math.abs(y - cy) - (h - r);
  const ox = Math.max(dx, 0);
  const oy = Math.max(dy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - r;
}

function distToCircle(x, y, cx, cy, r) {
  return Math.hypot(x - cx, y - cy) - r;
}

// The mark: three linked dots (a small "knowledge graph") inside a rounded
// square, on a teal gradient. Anti-aliased by supersampling.
function pixelColor(x, y, size) {
  const cx = size / 2;
  const cy = size / 2;
  const h = size * 0.44;
  const r = size * 0.14;
  const bg = roundedSquare(x, y, cx, cy, h, r);

  const dots = [
    [0.42, 0.38, 0.10], // left
    [0.58, 0.38, 0.10], // right
    [0.50, 0.62, 0.13], // bottom
  ].map(([fx, fy, fr]) => distToCircle(x, y, fx * size, fy * size, fr * size));

  const link = (() => {
    // thin connectors between the dots
    let d = Number.POSITIVE_INFINITY;
    for (let i = 0; i < dots.length; i++) {
      for (let j = i + 1; j < dots.length; j++) {
        const [ax, ay] = [[0.42, 0.38], [0.58, 0.38], [0.50, 0.62]][i];
        const [bx, by] = [[0.42, 0.38], [0.58, 0.38], [0.50, 0.62]][j];
        d = Math.min(d, segDist(x, y, ax * size, ay * size, bx * size, by * size));
      }
    }
    return d;
  })();

  // smooth alpha from the signed distances (coverage ~ 1px)
  const aa = (d) => Math.min(1, Math.max(0, 0.5 - d));

  const onBg = aa(bg);
  const onDot = Math.min(1, ...dots.map(aa)) * 0.95;
  const onLink = aa(link) * 0.8;

  if (onBg <= 0) return [0, 0, 0, 0];

  // teal gradient background
  const t = (x + y) / (2 * size);
  const rC = 14 + (21 - 14) * t;
  const gC = 116 + (78 - 116) * t;
  const bC = 144 + (109 - 144) * t;

  const white = Math.max(onDot, onLink);
  const cr = rC + (255 - rC) * white;
  const cg = gC + (255 - gC) * white;
  const cb = bC + (255 - bC) * white;
  return [Math.round(cr), Math.round(cg), Math.round(cb), 255];
}

// distance from (x, y) to segment ab
function segDist(x, y, ax, ay, bx, by) {
  const vx = bx - ax;
  const vy = by - ay;
  const wx = x - ax;
  const wy = y - ay;
  const len2 = vx * vx + vy * vy || 1;
  const t = Math.max(0, Math.min(1, (wx * vx + wy * vy) / len2));
  return Math.hypot(wx - t * vx, wy - t * vy);
}

function render(size) {
  const ss = 4; // supersample factor
  const raw = Buffer.alloc(size * (size * 4 + 1));
  let o = 0;
  for (let y = 0; y < size; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < size; x++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sy = 0; sy < ss; sy++) {
        for (let sx = 0; sx < ss; sx++) {
          const px = x + (sx + 0.5) / ss;
          const py = y + (sy + 0.5) / ss;
          const [pr, pg, pb, pa] = pixelColor(px, py, size);
          r += pr * pa; g += pg * pa; b += pb * pa; a += pa;
        }
      }
      const n = ss * ss;
      raw[o++] = Math.round(r / a || 0);
      raw[o++] = Math.round(g / a || 0);
      raw[o++] = Math.round(b / a || 0);
      raw[o++] = Math.round(a / n);
    }
  }
  return raw;
}

// ---- PNG encoding ----------------------------------------------------------

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size) {
  const raw = render(size);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', idat),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---- ICO / ICNS ------------------------------------------------------------

function ico(pngBuf) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(1, 4); // count
  const entry = Buffer.alloc(16);
  entry[0] = 0; // width 256
  entry[1] = 0; // height 256
  entry[2] = 0; // palette
  entry[3] = 0;
  entry.writeUInt16LE(1, 4); // planes
  entry.writeUInt16LE(32, 6); // bit count
  entry.writeUInt32LE(pngBuf.length, 8);
  entry.writeUInt32LE(22, 12); // offset
  return Buffer.concat([header, entry, pngBuf]);
}

function icns(pngs) {
  const chunks = [];
  for (const [type, size] of [['ic07', 128], ['ic08', 256], ['ic09', 512], ['ic10', 1024]]) {
    if (pngs[size]) {
      const h = Buffer.alloc(8);
      h.write(type, 0, 'ascii');
      h.writeUInt32BE(pngs[size].length + 8, 4);
      chunks.push(Buffer.concat([h, pngs[size]]));
    }
  }
  const head = Buffer.alloc(8);
  head.write('icns', 0, 'ascii');
  const total = chunks.reduce((n, c) => n + c.length, 8);
  head.writeUInt32BE(total, 4);
  return Buffer.concat([head, ...chunks]);
}

// ---- emit ------------------------------------------------------------------

mkdirSync(outDir, { recursive: true });

const pngs = {};
const files = [
  [32, '32x32.png'],
  [128, '128x128.png'],
  [256, '128x128@2x.png'],
  [512, 'icon.png'],
];
for (const [size, name] of files) {
  pngs[size] = png(size);
  writeFileSync(join(outDir, name), pngs[size]);
  console.log(`wrote ${name} (${size}x${size})`);
}
writeFileSync(join(outDir, 'icon.ico'), ico(pngs[256]));
console.log('wrote icon.ico');
writeFileSync(join(outDir, 'icon.icns'), icns(pngs));
console.log('wrote icon.icns');
