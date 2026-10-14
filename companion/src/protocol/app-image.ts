// Reads the header of a compiled NeoTime image: its kind, id, name and launcher group, without checking its code.
// The layout is runtime/src/image.rs in the NeoTime tree; keep the two in step.

export type ImageKind = 'app' | 'face' | 'library';

// `group` is the launcher group the app declares, or empty when it declares none.
export type ImageHeader = { kind: ImageKind; id: string; name: string; group: string; hasIcon: boolean; bytes: number };

export class AppImageError extends Error {}

const MAGIC = 'NTAP';
const FORMAT_VERSION = 2;
const HEADER_BYTES = 20;
const SECTION_ENTRY_BYTES = 12;
const TABLE_ENTRY_BYTES = 4;
const STRINGS_SECTION = 1;
const LAUNCHER_SECTION = 11;
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

  let strings: { section: Uint8Array; count: number } | null = null;
  let groupString: number | null = null;
  for (let index = 0; index < sections; index++) {
    const entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
    if (entry + SECTION_ENTRY_BYTES > bytes.length) break;
    const number = view.getUint16(entry, true);
    const offset = view.getUint32(entry + 4, true);
    const length = view.getUint32(entry + 8, true);
    if (number === STRINGS_SECTION) {
      strings = { section: bytes.subarray(offset, offset + length), count: view.getUint16(entry + 2, true) };
    } else if (number === LAUNCHER_SECTION && length >= 2 && offset + 2 <= bytes.length) {
      groupString = view.getUint16(offset, true);
    }
  }
  if (!strings) {
    throw new AppImageError('That image has no strings, so it names no app.');
  }
  const { section, count } = strings;
  const string = (which: number) => readString(section, count, which);
  return {
    kind,
    id: string(idString),
    name: string(nameString),
    group: groupString === null ? '' : string(groupString),
    hasIcon: (bytes[7] & 1) !== 0,
    bytes: bytes.length,
  };
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
