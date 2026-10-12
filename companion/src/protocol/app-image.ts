// Reads the header of a compiled NeoTime image: its kind, id and name, without checking its code.
// The layout is runtime/src/image.rs in the NeoTime tree; keep the two in step.

export type ImageKind = 'app' | 'face' | 'library';

export type ImageHeader = { kind: ImageKind; id: string; name: string; hasIcon: boolean; bytes: number };

export class AppImageError extends Error {}

const MAGIC = 'NTAP';
const FORMAT_VERSION = 1;
const HEADER_BYTES = 20;
const SECTION_ENTRY_BYTES = 12;
const TABLE_ENTRY_BYTES = 4;
const STRINGS_SECTION = 1;
const KINDS: ImageKind[] = ['app', 'face', 'library'];

export function readImageHeader(bytes: Uint8Array): ImageHeader {
  if (bytes.length < HEADER_BYTES || String.fromCharCode(...bytes.subarray(0, 4)) !== MAGIC) {
    throw new AppImageError('That file is not a NeoTime app image.');
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint16(4, true) !== FORMAT_VERSION) {
    throw new AppImageError('That image was made for another version of NeoTime.');
  }
  const kind = KINDS[bytes[6]];
  if (!kind) {
    throw new AppImageError('That image is of a kind this app does not know.');
  }
  const sections = view.getUint16(10, true);
  const idString = view.getUint16(16, true);
  const nameString = view.getUint16(18, true);

  for (let index = 0; index < sections; index++) {
    const entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
    if (entry + SECTION_ENTRY_BYTES > bytes.length) break;
    if (view.getUint16(entry, true) !== STRINGS_SECTION) continue;
    const count = view.getUint16(entry + 2, true);
    const offset = view.getUint32(entry + 4, true);
    const length = view.getUint32(entry + 8, true);
    const section = bytes.subarray(offset, offset + length);
    const string = (which: number) => readString(section, count, which);
    return { kind, id: string(idString), name: string(nameString), hasIcon: (bytes[7] & 1) !== 0, bytes: bytes.length };
  }
  throw new AppImageError('That image has no strings, so it names no app.');
}

function readString(section: Uint8Array, count: number, index: number): string {
  if (index >= count) {
    throw new AppImageError('That image names a string it does not have.');
  }
  const view = new DataView(section.buffer, section.byteOffset, section.byteLength);
  const data = section.subarray(count * TABLE_ENTRY_BYTES);
  const start = view.getUint16(index * TABLE_ENTRY_BYTES, true);
  const length = view.getUint16(index * TABLE_ENTRY_BYTES + 2, true);
  if (start + length > data.length) {
    throw new AppImageError('That image is cut short.');
  }
  return new TextDecoder().decode(data.subarray(start, start + length));
}
