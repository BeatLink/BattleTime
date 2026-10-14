import { describe, expect, it } from '@jest/globals';
import { unzlibSync } from 'fflate';
import { crc32 } from 'node:zlib';

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';
import { BUNDLED_IMAGES } from '@/bundles/watch-apps';

import { libraryIconUris, maskToPng, readLibraryIcons, type IconMask } from './library-icons';

// A library image holding only a resources section with the given resources.
function libraryWith(resources: Uint8Array[]): Uint8Array {
  const tableBytes = resources.length * 4;
  const dataBytes = resources.reduce((sum, resource) => sum + resource.length, 0);
  const sectionOffset = 20 + 12;
  const image = new Uint8Array(sectionOffset + tableBytes + dataBytes);
  const view = new DataView(image.buffer);
  image.set([0x4e, 0x54, 0x41, 0x50]);
  view.setUint16(10, 1, true);
  view.setUint16(20, 7, true);
  view.setUint16(22, resources.length, true);
  view.setUint32(24, sectionOffset, true);
  view.setUint32(28, tableBytes + dataBytes, true);
  let at = 0;
  resources.forEach((resource, index) => {
    view.setUint16(sectionOffset + index * 4, at, true);
    view.setUint16(sectionOffset + index * 4 + 2, resource.length, true);
    image.set(resource, sectionOffset + tableBytes + at);
    at += resource.length;
  });
  return image;
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
  it('unpacks every icon in the bundled library as a 32 pixel mask', () => {
    const library = BUNDLED_IMAGES.find((image) => image.file === 'shared.ntlib')!;
    const icons = readLibraryIcons(base64ToBytes(library.base64));
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
    const icons = readLibraryIcons(libraryWith([mask, colour, short, long]));
    expect(icons[0]).toEqual({ width: 2, height: 2, coverage: Uint8Array.from([0, 0, 0, 255]) });
    expect(icons.slice(1)).toEqual([null, null, null]);
  });

  it('finds no icons in an image without a resources section', () => {
    expect(readLibraryIcons(new Uint8Array(4))).toEqual([]);
    const noResources = libraryWith([]);
    new DataView(noResources.buffer).setUint16(20, 1, true);
    expect(readLibraryIcons(noResources)).toEqual([]);
    const cutShort = new Uint8Array(22);
    new DataView(cutShort.buffer).setUint16(10, 3, true);
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

  it('gives a data URI per icon, and none without a library in the bundle', () => {
    const uris = libraryIconUris();
    expect(uris[0]).toMatch(/^data:image\/png;base64,/);
    expect(libraryIconUris()).toBe(uris);
    expect(libraryIconUris([{ file: 'timer.ntapp', base64: '' }])).toEqual([]);
    const colourOnly = libraryWith([Uint8Array.from([0, 0, 1, 0, 1, 0, 1, 0, 0])]);
    expect(libraryIconUris([{ file: 'shared.ntlib', base64: bytesToBase64(colourOnly) }])).toEqual([null]);
  });
});
