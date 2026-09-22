// ASTeam — SMSClient icon generator (zero-dependency, pure Node)
// Generates: src-tauri/icons/{32x32.png,128x128.png,128x128@2x.png,icon.ico}
import zlib from 'node:zlib';
import fs from 'node:fs';
import path from 'node:path';

/* ---------- PNG encoder ---------- */
const CRC_TABLE = [];
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  CRC_TABLE[n] = c >>> 0;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}
function encodePNG(size, rgba) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;  // bit depth
  ihdr[9] = 6;  // RGBA
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y++) {
    raw[y * stride] = 0; // filter: none
    rgba.copy(raw, y * stride + 1, y * size * 4, (y + 1) * size * 4);
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([sig, chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', Buffer.alloc(0))]);
}

/* ---------- brand artwork: rounded message bubble + tail + 3 dots ---------- */
// designed on a 64x64 logical grid, nearest-sampled to target size
function drawIcon(size) {
  const buf = Buffer.alloc(size * size * 4);
  const G = 64;
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      const x = (px * G) / size;
      const y = (py * G) / size;
      let r, g, b, a;
      // background: vertical indigo gradient with rounded-rect mask (squircle-ish)
      const m = 6; // corner radius
      const inCorner =
        (x < m && y < m && (x - m) ** 2 + (y - m) ** 2 > m * m) ||
        (x > G - m && y < m && (x - (G - m)) ** 2 + (y - m) ** 2 > m * m) ||
        (x < m && y > G - m && (x - m) ** 2 + (y - (G - m)) ** 2 > m * m) ||
        (x > G - m && y > G - m && (x - (G - m)) ** 2 + (y - (G - m)) ** 2 > m * m);
      if (inCorner) {
        r = g = b = a = 0;
      } else {
        const t = y / G;
        r = Math.round(0x4f + (0x63 - 0x4f) * t);
        g = Math.round(0x46 + (0x66 - 0x46) * t);
        b = Math.round(0xe5 + (0xf1 - 0xe5) * t);
        a = 255;
      }
      // white bubble body (rounded rect 14..50 x 16..44, radius 6)
      const bx0 = 14, bx1 = 50, by0 = 16, by1 = 42, br = 6;
      let inBubble =
        x >= bx0 && x <= bx1 && y >= by0 && y <= by1 &&
        !((x < bx0 + br && y < by0 + br && (x - bx0 - br) ** 2 + (y - by0 - br) ** 2 > br * br) ||
          (x > bx1 - br && y < by0 + br && (x - (bx1 - br)) ** 2 + (y - by0 - br) ** 2 > br * br) ||
          (x < bx0 + br && y > by1 - br && (x - bx0 - br) ** 2 + (y - (by1 - br)) ** 2 > br * br) ||
          (x > bx1 - br && y > by1 - br && (x - (bx1 - br)) ** 2 + (y - (by1 - br)) ** 2 > br * br));
      // tail: triangle (18,42)-(28,42)-(18,54)
      if (!inBubble && y >= by1 && y <= 54 && x >= 18 && x <= 28) {
        const slope = (y - by1) / 12; // 0..1
        if (x <= 28 - slope * 10) inBubble = true;
      }
      if (inBubble) { r = 255; g = 255; b = 255; a = 255; }
      // 3 indigo dots inside bubble
      for (const cx of [23, 32, 41]) {
        if ((x - cx) ** 2 + (y - 29) ** 2 <= 3.2 ** 2) { r = 0x4f; g = 0x46; b = 0xe5; a = 255; }
      }
      const o = (py * size + px) * 4;
      buf[o] = r; buf[o + 1] = g; buf[o + 2] = b; buf[o + 3] = a;
    }
  }
  return buf;
}

/* ---------- ICO (PNG-embedded, Vista+) ---------- */
function buildICO(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  let offset = 6 + 16 * entries.length;
  const dir = [];
  for (const { size, png } of entries) {
    const e = Buffer.alloc(16);
    e[0] = size >= 256 ? 0 : size;
    e[1] = size >= 256 ? 0 : size;
    e.writeUInt16LE(1, 4);
    e.writeUInt16LE(32, 6);
    e.writeUInt32LE(png.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += png.length;
    dir.push(e);
  }
  return Buffer.concat([header, ...dir, ...entries.map((e) => e.png)]);
}

/* ---------- emit ---------- */
const outDir = path.resolve('src-tauri/icons');
fs.mkdirSync(outDir, { recursive: true });

const specs = [
  ['32x32.png', 32],
  ['128x128.png', 128],
  ['128x128@2x.png', 256],
  ['icon.png', 512],
];
const built = {};
for (const [name, size] of specs) {
  const png = encodePNG(size, drawIcon(size));
  fs.writeFileSync(path.join(outDir, name), png);
  built[size] = png;
  console.log(`[OK] icons/${name} (${png.length} bytes)`);
}

const ico = buildICO([
  { size: 32, png: built[32] },
  { size: 128, png: built[128] },
  { size: 256, png: built[256] },
]);
fs.writeFileSync(path.join(outDir, 'icon.ico'), ico);
console.log(`[OK] icons/icon.ico (${ico.length} bytes)`);
console.log('icon generation complete.');
