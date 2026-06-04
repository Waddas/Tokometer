// One-shot dev tool: generates src-tauri/app-icon.png (1024x1024 Clawd pixel
// art, for `tauri icon`) and the three 64x64 tray icon variants with a status
// bubble (bubble radius = 1/3 icon radius, bottom-right). The icons are
// committed, so you only need this when regenerating them.
// Pure Node (zlib) PNG encoder — no dependencies.
//
//   node tools/gen_icons.mjs <path-to-claudepix-frame.json>
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const SRC = process.argv[2];
if (!SRC) {
  console.error("usage: node tools/gen_icons.mjs <path-to-claudepix-frame.json>");
  process.exit(1);
}
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ICONS_DIR = join(ROOT, "src-tauri", "icons");
mkdirSync(ICONS_DIR, { recursive: true });

// ---- minimal PNG encoder (RGBA8, filter 0) ----
const CRC_TABLE = new Int32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});
function crc32(buf) {
  let c = -1;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}
function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}
function encodePng(w, h, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc(h * (1 + w * 4));
  for (let y = 0; y < h; y++) rgba.copy(raw, y * (1 + w * 4) + 1, y * w * 4, (y + 1) * w * 4);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

class Canvas {
  constructor(w, h) {
    this.w = w;
    this.h = h;
    this.data = Buffer.alloc(w * h * 4);
  }
  fillRect(x0, y0, w, h, [r, g, b]) {
    for (let y = y0; y < y0 + h; y++)
      for (let x = x0; x < x0 + w; x++) {
        const i = (y * this.w + x) * 4;
        this.data[i] = r;
        this.data[i + 1] = g;
        this.data[i + 2] = b;
        this.data[i + 3] = 255;
      }
  }
  fillCircle(cx, cy, rad, rgb) {
    for (let y = cy - rad; y <= cy + rad; y++)
      for (let x = cx - rad; x <= cx + rad; x++)
        if (x >= 0 && y >= 0 && x < this.w && y < this.h && (x - cx) ** 2 + (y - cy) ** 2 <= rad * rad)
          this.fillRect(x, y, 1, 1, rgb);
  }
  save(path) {
    writeFileSync(path, encodePng(this.w, this.h, this.data));
    console.log(`wrote ${path}`);
  }
}

// ---- render Clawd (frame 0 of idle breathe) ----
const hexToRgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const art = JSON.parse(readFileSync(SRC, "utf8"));
const palette = art.palette.map((c) => (c === "transparent" ? null : hexToRgb(c)));
const grid = art.frames[0].grid;

function drawClawd(canvas, offset, cell) {
  for (let y = 0; y < 20; y++)
    for (let x = 0; x < 20; x++) {
      const rgb = palette[grid[y][x]];
      if (rgb) canvas.fillRect(offset + x * cell, offset + y * cell, cell, cell, rgb);
    }
}

// App icon: 1024x1024 (20 cells x 51px + 2px border)
const app = new Canvas(1024, 1024);
drawClawd(app, 2, 51);
app.save(join(ROOT, "src-tauri", "app-icon.png"));

// Tray icons: 64x64 (20 cells x 3px + 2px border) + status bubble bottom-right
const BUBBLES = {
  "tray-ok": [60, 200, 90],
  "tray-busy": [240, 180, 40],
  "tray-error": [220, 60, 60],
};
for (const [name, color] of Object.entries(BUBBLES)) {
  const c = new Canvas(64, 64);
  drawClawd(c, 2, 3);
  c.fillCircle(52, 52, 11, color);
  c.save(join(ICONS_DIR, `${name}.png`));
}
