// Generates the tray icons of tamias (app/src-tauri/src/tray.rs), one PNG per state:
//
//   node app/src-tauri/icons/tray/generate.mjs
//
// A traffic-light disc, 64x64 RGBA, anti-aliased by 8x8 supersampling, with a darker rim so it
// stays visible on light and dark menu bars and taskbars at 16-32 px. `neutral` (no provider
// set up) is a hollow ring, so it is not mistaken for `grey` (a stale provider). The icons are
// not macOS template images: the colour is the information.
//
// No dependencies: Node's zlib and a hand-written PNG encoder, so the output depends on the
// constants below only.

import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const SIZE = 64;
const SAMPLES = 8;
const CENTRE = SIZE / 2;
/** Outer radius of the disc, leaving a 4 px margin like other tray icons. */
const RADIUS = 28;
/** Width of the darker rim of a filled disc. */
const RIM = 4;
/** Width of the ring of `neutral`. */
const RING = 7;

/** Fill and rim colours, RGB. */
const ICONS = {
  green: { fill: [0x2e, 0xa0, 0x43], rim: [0x1a, 0x5e, 0x27] },
  yellow: { fill: [0xf2, 0xc1, 0x2e], rim: [0x8a, 0x6a, 0x0b] },
  red: { fill: [0xe0, 0x3c, 0x31], rim: [0x86, 0x1d, 0x16] },
  grey: { fill: [0x9a, 0xa0, 0xa6], rim: [0x4d, 0x52, 0x57] },
  neutral: { fill: null, rim: [0x80, 0x86, 0x8c] },
};

/** RGBA of the pixel (x, y) of an icon: the average of its subsamples. */
function pixel(icon, x, y) {
  let r = 0;
  let g = 0;
  let b = 0;
  let a = 0;
  for (let sy = 0; sy < SAMPLES; sy++) {
    for (let sx = 0; sx < SAMPLES; sx++) {
      const dx = x + (sx + 0.5) / SAMPLES - CENTRE;
      const dy = y + (sy + 0.5) / SAMPLES - CENTRE;
      const d = Math.hypot(dx, dy);
      if (d > RADIUS) continue;
      const inner = icon.fill === null ? RADIUS - RING : RADIUS - RIM;
      let colour;
      if (d > inner) colour = icon.rim;
      else if (icon.fill !== null) colour = icon.fill;
      else continue;
      r += colour[0];
      g += colour[1];
      b += colour[2];
      a += 1;
    }
  }
  if (a === 0) return [0, 0, 0, 0];
  const n = SAMPLES * SAMPLES;
  return [Math.round(r / a), Math.round(g / a), Math.round(b / a), Math.round((255 * a) / n)];
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes) {
  let c = 0xffffffff;
  for (const byte of bytes) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/** PNG of an icon: 8-bit RGBA, no interlace, filter 0 on every row. */
function png(icon) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(SIZE, 0);
  header.writeUInt32BE(SIZE, 4);
  header.set([8, 6, 0, 0, 0], 8);
  const raw = Buffer.alloc(SIZE * (1 + SIZE * 4));
  for (let y = 0; y < SIZE; y++) {
    const row = y * (1 + SIZE * 4);
    raw[row] = 0;
    for (let x = 0; x < SIZE; x++) raw.set(pixel(icon, x, y), row + 1 + x * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const dir = dirname(fileURLToPath(import.meta.url));
for (const [name, icon] of Object.entries(ICONS)) {
  writeFileSync(join(dir, `${name}.png`), png(icon));
}
