// Builds the smallest image the header reader and the mock watch accept: a header, a strings section and,
// when the app declares a launcher group, the launcher section naming it.
// The code sections and the CRC are left out, since only the watch checks those.

import type { ImageKind } from '@/protocol/app-image';

const KIND_NUMBER: Record<ImageKind, number> = { app: 0, face: 1, library: 2 };

export function testImage(id: string, name: string, kind: ImageKind = 'app', padding = 0, group?: string): Uint8Array {
  const strings = [id, name, ...(group === undefined ? [] : [group])].map((text) => new TextEncoder().encode(text));
  const sectionCount = group === undefined ? 1 : 2;
  const tableBytes = strings.length * 4;
  const dataBytes = strings.reduce((sum, text) => sum + text.length, 0);
  const sectionOffset = 20 + 12 * sectionCount;
  const launcherOffset = sectionOffset + tableBytes + dataBytes;
  const out = new Uint8Array(launcherOffset + (group === undefined ? 0 : 2) + padding);
  const view = new DataView(out.buffer);
  out.set(new TextEncoder().encode('NTAP'), 0);
  view.setUint16(4, 2, true);
  out[6] = KIND_NUMBER[kind];
  view.setUint16(10, sectionCount, true);
  view.setUint16(16, 0, true);
  view.setUint16(18, 1, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, strings.length, true);
  view.setUint32(24, sectionOffset, true);
  view.setUint32(28, tableBytes + dataBytes, true);
  if (group !== undefined) {
    view.setUint16(32, 11, true);
    view.setUint16(34, 1, true);
    view.setUint32(36, launcherOffset, true);
    view.setUint32(40, 2, true);
    view.setUint16(launcherOffset, 2, true);
  }
  let start = 0;
  strings.forEach((text, index) => {
    view.setUint16(sectionOffset + index * 4, start, true);
    view.setUint16(sectionOffset + index * 4 + 2, text.length, true);
    out.set(text, sectionOffset + tableBytes + start);
    start += text.length;
  });
  return out;
}
