import { describe, expect, it } from '@jest/globals';
import { unzlibSync } from 'fflate';
import { crc32 } from 'node:zlib';

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';
import { SYSTEM_FILES } from '@/bundles/system-files';

import { ICONS_PATH, libraryIconUris, maskToPng, readLibraryIcons, type IconMask } from './library-icons';

// An icons file holding these icons, as runtime/src/library_icons.rs writes it.
function iconsFileWith(icons: Uint8Array[]): Uint8Array {
  const tableEnd = 4 + (icons.length + 1) * 4;
  const file = new Uint8Array(tableEnd + icons.reduce((sum, icon) => sum + icon.length, 0));
  const view = new DataView(file.buffer);
  view.setUint16(0, icons.length, true);
  let at = tableEnd;
  icons.forEach((icon, index) => {
    view.setUint32(4 + index * 4, at, true);
    file.set(icon, at);
    at += icon.length;
  });
  view.setUint32(4 + icons.length * 4, at, true);
  return file;
}

// Reads the RGBA pixels back out of a PNG maskToPng wrote.
function pngPixels(png: Uint8Array): { width: number; height: number; raw: Uint8Array } {
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  const width = view.getUint32(16);
  const height = view.getUint32(20);
  const idatLength = view.getUint32(33);
  expect(String.fromCharCode(...png.subarray(37, 41))).toBe('IDAT');
  return { width, height, raw: unzlibSync(png.subarray(41, 41 + idatLength)) };
}

describe('the shared library icons', () => {
  it('unpacks every icon in the bundled icons file as a 32 pixel mask', () => {
    const file = SYSTEM_FILES.find((system) => system.path === ICONS_PATH)!;
    const icons = readLibraryIcons(base64ToBytes(file.base64));
    expect(icons.length).toBeGreaterThan(0);
    for (const icon of icons) {
      expect(icon).not.toBeNull();
      expect(icon!.width).toBe(32);
      expect(icon!.coverage).toHaveLength(32 * 32);
    }
    // The play icon has some solid pixels and some clear ones.
    const play = icons[32]!;
    expect(play.coverage.some((value) => value === 255)).toBe(true);
    expect(play.coverage.some((value) => value === 0)).toBe(true);
  });

  it('expands runs, and leaves out colour images and runs that do not fit', () => {
    const mask = Uint8Array.from([1, 0, 2, 0, 2, 0, 3, 0, 1, 255]);
    const colour = Uint8Array.from([0, 0, 1, 0, 1, 0, 1, 0xff, 0xff]);
    const short = Uint8Array.from([1, 0, 2, 0, 2, 0, 3, 0]);
    const long = Uint8Array.from([1, 0, 1, 0, 1, 0, 9, 0]);
    const icons = readLibraryIcons(iconsFileWith([mask, colour, short, long]));
    expect(icons[0]).toEqual({ width: 2, height: 2, coverage: Uint8Array.from([0, 0, 0, 255]) });
    expect(icons.slice(1)).toEqual([null, null, null]);
  });

  it('finds no icons in an empty file or one whose table is cut short', () => {
    expect(readLibraryIcons(new Uint8Array(2))).toEqual([]);
    expect(readLibraryIcons(iconsFileWith([]))).toEqual([]);
    const cutShort = new Uint8Array(8);
    new DataView(cutShort.buffer).setUint16(0, 3, true);
    expect(readLibraryIcons(cutShort)).toEqual([]);
  });

  it('writes a mask as a white PNG whose alpha is the coverage', () => {
    const mask: IconMask = { width: 2, height: 1, coverage: Uint8Array.from([0, 200]) };
    const png = maskToPng(mask);
    expect(Array.from(png.subarray(0, 8))).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const { width, height, raw } = pngPixels(png);
    expect([width, height]).toEqual([2, 1]);
    expect(Array.from(raw)).toEqual([0, 255, 255, 255, 0, 255, 255, 255, 200]);
    // The IHDR chunk's checksum matches Node's own CRC-32, as any PNG reader checks it.
    expect(new DataView(png.buffer).getUint32(29)).toBe(crc32(png.subarray(12, 29)));
  });

  it('gives a data URI per icon, and none without an icons file in the bundle', () => {
    const uris = libraryIconUris();
    expect(uris[0]).toMatch(/^data:image\/png;base64,/);
    expect(libraryIconUris()).toBe(uris);
    expect(libraryIconUris([{ path: '/system/accelerometer/bma425', base64: '' }])).toEqual([]);
    const colourOnly = iconsFileWith([Uint8Array.from([0, 0, 1, 0, 1, 0, 1, 0, 0])]);
    expect(libraryIconUris([{ path: ICONS_PATH, base64: bytesToBase64(colourOnly) }])).toEqual([null]);
  });
});
