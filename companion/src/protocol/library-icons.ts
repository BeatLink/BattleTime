// The shared library's icons as pictures the screens can show, drawn from the icons file the companion puts on
// the watch at /system/icons, which it carries among its system files.
//
// The file's layout is runtime/src/library_icons.rs in the BattleTime tree: the icon count (2 bytes), 2 reserved
// bytes, each icon's start and the file's end (4 bytes each), then the icons. Each icon is a mask, laid out as
// runtime/src/resources.rs says: runs of (count, coverage), which this file turns into a white PNG whose alpha
// is the coverage, so a screen can tint it in any colour as the watch does.

import { zlibSync } from 'fflate';

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';
import { SYSTEM_FILES, type BundledSystemFile } from '@/bundles/system-files';

// ============================================================================
// Reading the icons file
// ============================================================================

export const ICONS_PATH = '/system/icons';
const HEADER_BYTES = 4;
const PLACE_BYTES = 4;
const RESOURCE_HEADER_BYTES = 6;
const MASK_FORMAT = 1;
const MASK_RUN_BYTES = 2;

// One icon: its size and a coverage from 0 (clear) to 255 (solid) for each pixel, row by row.
export type IconMask = { width: number; height: number; coverage: Uint8Array };

// Unpacks one resource, or gives null for a colour image or one whose runs do not cover it exactly.
function readMask(resource: Uint8Array): IconMask | null {
  if (resource.length < RESOURCE_HEADER_BYTES || resource[0] !== MASK_FORMAT) {
    return null;
  }
  const width = resource[2] | (resource[3] << 8);
  const height = resource[4] | (resource[5] << 8);
  const coverage = new Uint8Array(width * height);
  let filled = 0;
  for (let run = RESOURCE_HEADER_BYTES; run + MASK_RUN_BYTES <= resource.length; run += MASK_RUN_BYTES) {
    const count = resource[run];
    if (filled + count > coverage.length) {
      return null;
    }
    coverage.fill(resource[run + 1], filled, filled + count);
    filled += count;
  }
  return filled === coverage.length ? { width, height, coverage } : null;
}

// Every icon in the icons file, in order, with null where one is not a mask; none when the file is cut short.
export function readLibraryIcons(file: Uint8Array): (IconMask | null)[] {
  if (file.length < HEADER_BYTES) {
    return [];
  }
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  const count = view.getUint16(0, true);
  if (HEADER_BYTES + (count + 1) * PLACE_BYTES > file.length) {
    return [];
  }
  const place = (which: number) => view.getUint32(HEADER_BYTES + which * PLACE_BYTES, true);
  return Array.from({ length: count }, (_unused, which) => readMask(file.subarray(place(which), place(which + 1))));
}

// ============================================================================
// Writing a PNG
// ============================================================================

const CRC_TABLE = Array.from({ length: 256 }, (_unused, byte) => {
  let value = byte;
  for (let bit = 0; bit < 8; bit++) {
    value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  return value >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type: string, body: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + body.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, body.length);
  for (let index = 0; index < 4; index++) {
    out[4 + index] = type.charCodeAt(index);
  }
  out.set(body, 8);
  view.setUint32(8 + body.length, crc32(out.subarray(4, 8 + body.length)));
  return out;
}

// A white RGBA PNG whose alpha is the mask's coverage.
export function maskToPng(mask: IconMask): Uint8Array {
  const header = new Uint8Array(13);
  const headerView = new DataView(header.buffer);
  headerView.setUint32(0, mask.width);
  headerView.setUint32(4, mask.height);
  header[8] = 8; // Bits per channel.
  header[9] = 6; // Colour with alpha.
  const rowBytes = 1 + mask.width * 4;
  const raw = new Uint8Array(rowBytes * mask.height);
  for (let row = 0; row < mask.height; row++) {
    for (let column = 0; column < mask.width; column++) {
      const pixel = row * rowBytes + 1 + column * 4;
      raw.fill(255, pixel, pixel + 3);
      raw[pixel + 3] = mask.coverage[row * mask.width + column];
    }
  }
  const parts = [
    Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', zlibSync(raw)),
    chunk('IEND', new Uint8Array(0)),
  ];
  const png = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let at = 0;
  for (const part of parts) {
    png.set(part, at);
    at += part.length;
  }
  return png;
}

// ============================================================================
// The bundled icons
// ============================================================================

let bundledUris: (string | null)[] | null = null;

// Each library icon as a PNG data URI, in library order, or null where the bundle has no picture for it.
export function libraryIconUris(files: BundledSystemFile[] = SYSTEM_FILES): (string | null)[] {
  const cached = files === SYSTEM_FILES ? bundledUris : null;
  if (cached) {
    return cached;
  }
  const icons = files.find((file) => file.path === ICONS_PATH);
  const masks = icons ? readLibraryIcons(base64ToBytes(icons.base64)) : [];
  const uris = masks.map((mask) => (mask ? `data:image/png;base64,${bytesToBase64(maskToPng(mask))}` : null));
  if (files === SYSTEM_FILES) {
    bundledUris = uris;
  }
  return uris;
}
