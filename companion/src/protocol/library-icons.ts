// The shared library's icons as pictures the screens can show, drawn from the library image the setup bundle carries.
//
// The layout of the image and its resources section is runtime/src/image.rs and runtime/src/resources.rs in
// the BattleTime tree. Each icon is a mask: runs of (count, coverage), which this file turns into a white PNG
// whose alpha is the coverage, so a screen can tint it in any colour as the watch does.

import { zlibSync } from 'fflate';

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';
import { BUNDLED_IMAGES, type BundledImage } from '@/bundles/watch-apps';

// ============================================================================
// Reading the library image
// ============================================================================

const HEADER_BYTES = 20;
const SECTION_ENTRY_BYTES = 12;
const TABLE_ENTRY_BYTES = 4;
const RESOURCES_SECTION = 7;
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

// Every image in the library's resources section, in order, with null where one is not a mask.
export function readLibraryIcons(image: Uint8Array): (IconMask | null)[] {
  if (image.length < HEADER_BYTES) {
    return [];
  }
  const view = new DataView(image.buffer, image.byteOffset, image.byteLength);
  const sections = view.getUint16(10, true);
  for (let index = 0; index < sections; index++) {
    const entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
    if (entry + SECTION_ENTRY_BYTES > image.length) {
      break;
    }
    if (view.getUint16(entry, true) !== RESOURCES_SECTION) {
      continue;
    }
    const count = view.getUint16(entry + 2, true);
    const offset = view.getUint32(entry + 4, true);
    const length = view.getUint32(entry + 8, true);
    const section = image.subarray(offset, offset + length);
    const table = new DataView(section.buffer, section.byteOffset, section.byteLength);
    const data = section.subarray(count * TABLE_ENTRY_BYTES);
    return Array.from({ length: count }, (_unused, which) => {
      const start = table.getUint16(which * TABLE_ENTRY_BYTES, true);
      const size = table.getUint16(which * TABLE_ENTRY_BYTES + 2, true);
      return readMask(data.subarray(start, start + size));
    });
  }
  return [];
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
// The bundled library's icons
// ============================================================================

let bundledUris: (string | null)[] | null = null;

// Each library icon as a PNG data URI, in library order, or null where the bundle has no picture for it.
export function libraryIconUris(bundle: BundledImage[] = BUNDLED_IMAGES): (string | null)[] {
  const cached = bundle === BUNDLED_IMAGES ? bundledUris : null;
  if (cached) {
    return cached;
  }
  const library = bundle.find((image) => image.file.endsWith('.ntlib'));
  const masks = library ? readLibraryIcons(base64ToBytes(library.base64)) : [];
  const uris = masks.map((mask) => (mask ? `data:image/png;base64,${bytesToBase64(maskToPng(mask))}` : null));
  if (bundle === BUNDLED_IMAGES) {
    bundledUris = uris;
  }
  return uris;
}
