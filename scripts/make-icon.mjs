#!/usr/bin/env node
/**
 * Generates `apps/desktop/src-tauri/icons/icon.ico` without any image dependency.
 *
 * Tauri's Windows build embeds a resource icon even when bundling is off, so the file must exist. This
 * script is the source of truth: run `node scripts/make-icon.mjs` after changing the mark.
 *
 * Format: 32x32, 32bpp BGRA + AND mask, single image — the minimum Windows accepts.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 32;
const BACKGROUND = [0x15, 0x11, 0x0f, 0xff]; // #0f1115, ABGR order below
const ACCENT = [0xa0, 0xd3, 0x7d, 0xff]; // #7dd3a0

const pixels = Buffer.alloc(SIZE * SIZE * 4);
for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    // A diamond: |dx| + |dy| <= 13, with a one-pixel notch for a mark that reads at 16px.
    const distance = Math.abs(x - 15.5) + Math.abs(y - 15.5);
    const color = distance <= 13 ? ACCENT : BACKGROUND;
    const row = SIZE - 1 - y; // BMP rows are bottom-up.
    const offset = (row * SIZE + x) * 4;
    pixels[offset] = color[0];
    pixels[offset + 1] = color[1];
    pixels[offset + 2] = color[2];
    pixels[offset + 3] = color[3];
  }
}

const andMask = Buffer.alloc(SIZE * 4); // 32 rows of 4 bytes, all zero = fully opaque.
const header = Buffer.alloc(40);
header.writeUInt32LE(40, 0); // biSize
header.writeInt32LE(SIZE, 4); // biWidth
header.writeInt32LE(SIZE * 2, 8); // biHeight (image + mask)
header.writeUInt16LE(1, 12); // biPlanes
header.writeUInt16LE(32, 14); // biBitCount
header.writeUInt32LE(0, 16); // biCompression
header.writeUInt32LE(pixels.length + andMask.length, 20); // biSizeImage
header.writeInt32LE(0, 24);
header.writeInt32LE(0, 28);
header.writeUInt32LE(0, 32);
header.writeUInt32LE(0, 36);

const image = Buffer.concat([header, pixels, andMask]);

const directory = Buffer.alloc(6);
directory.writeUInt16LE(0, 0); // reserved
directory.writeUInt16LE(1, 2); // type: icon
directory.writeUInt16LE(1, 4); // image count

const entry = Buffer.alloc(16);
entry[0] = SIZE;
entry[1] = SIZE;
entry[2] = 0; // palette size
entry[3] = 0; // reserved
entry.writeUInt16LE(1, 4); // planes
entry.writeUInt16LE(32, 6); // bit count
entry.writeUInt32LE(image.length, 8);
entry.writeUInt32LE(22, 12); // offset

const target = fileURLToPath(new URL("../apps/desktop/src-tauri/icons/icon.ico", import.meta.url));
mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, Buffer.concat([directory, entry, image]));
console.log(`wrote ${target} (${22 + image.length} bytes)`);
