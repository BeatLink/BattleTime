// An app's saved state, `/apps/<id>/state`, as the watch's runtime writes it, for builds without the Rust core's sync.
// It mirrors core/src/sync/state_file.rs; docs/sync.md describes the format, so keep the three in step.
//
// The file names every value, so it reads without the app's image: "NTST", version 1, a count, then for
// each value its key, type, array length, element width and little-endian bytes.

import { FileMissingError, type FileService } from './file-service';

// ============================================================================
// The format
// ============================================================================

const MAGIC = [0x4e, 0x54, 0x53, 0x54];
const VERSION = 1;
const HEADER_BYTES = 6;

// The type byte of each value: plain number, switch, local time in ms since 1970, ms, colour, string number.
export const VALUE_KIND = { int: 0, bool: 1, instant: 2, duration: 3, color: 4, text: 5 } as const;

export type ValueKind = keyof typeof VALUE_KIND;

const KIND_OF_BYTE = Object.fromEntries(Object.entries(VALUE_KIND).map(([name, byte]) => [byte, name])) as Record<
  number,
  ValueKind
>;

// One saved variable; `length` is 0 for a single value and the array's length otherwise.
export type StateEntry = { key: string; kind: ValueKind; length: number; width: 1 | 2 | 4 | 8; values: bigint[] };

export class AppStateError extends Error {}

// What an app's state looks like as JSON: numbers, switches, lists, and records as objects of lists.
export type AppStateJson = Record<string, unknown>;

// ============================================================================
// Reading and writing the bytes
// ============================================================================

export function decodeAppState(bytes: Uint8Array): StateEntry[] {
  if (bytes.length < HEADER_BYTES || MAGIC.some((byte, index) => bytes[index] !== byte) || bytes[4] !== VERSION) {
    throw new AppStateError("not an app's saved state");
  }
  const short = () => new AppStateError('the saved state stops part way');
  const entries: StateEntry[] = [];
  let at = HEADER_BYTES;
  for (let count = 0; count < bytes[5]; count++) {
    if (at >= bytes.length) throw short();
    const keyLength = bytes[at];
    const headerAt = at + 1 + keyLength;
    if (headerAt + 4 > bytes.length) throw short();
    const key = new TextDecoder().decode(bytes.subarray(at + 1, headerAt));
    const kind = KIND_OF_BYTE[bytes[headerAt]];
    if (kind === undefined) {
      throw new AppStateError('the saved state has a value of no known type');
    }
    const length = bytes[headerAt + 1] | (bytes[headerAt + 2] << 8);
    const width = bytes[headerAt + 3];
    if (width !== 1 && width !== 2 && width !== 4 && width !== 8) {
      throw new AppStateError(`the saved state stores a value in ${width} bytes`);
    }
    const start = headerAt + 4;
    const end = start + Math.max(length, 1) * width;
    if (end > bytes.length) throw short();
    const values: bigint[] = [];
    for (let place = start; place < end; place += width) {
      values.push(readPacked(bytes, place, width));
    }
    entries.push({ key, kind, length, width, values });
    at = end;
  }
  return entries;
}

export function encodeAppState(entries: StateEntry[]): Uint8Array {
  if (entries.length > 255) {
    throw new AppStateError('too many values for one saved state');
  }
  const parts: number[] = [...MAGIC, VERSION, entries.length];
  for (const entry of entries) {
    const key = new TextEncoder().encode(entry.key);
    if (key.length > 255) {
      throw new AppStateError(`the key ${entry.key} is too long`);
    }
    parts.push(key.length, ...key, VALUE_KIND[entry.kind], entry.length & 0xff, entry.length >> 8, entry.width);
    for (const value of entry.values) {
      if (!fits(value, entry.width)) {
        throw new AppStateError(`${entry.key} cannot hold ${value}`);
      }
      parts.push(...packed(value, entry.width));
    }
  }
  return Uint8Array.from(parts);
}

// Byte by byte with plain bigint arithmetic, since not every JavaScript engine the app runs on has DataView's 64-bit methods.
function readPacked(bytes: Uint8Array, at: number, width: number): bigint {
  let value = 0n;
  for (let place = width - 1; place >= 0; place--) {
    value = (value << 8n) | BigInt(bytes[at + place]);
  }
  return BigInt.asIntN(width * 8, value);
}

function packed(value: bigint, width: number): number[] {
  let rest = BigInt.asUintN(width * 8, value);
  const out: number[] = [];
  for (let place = 0; place < width; place++) {
    out.push(Number(rest & 0xffn));
    rest >>= 8n;
  }
  return out;
}

function fits(value: bigint, width: number): boolean {
  const limit = 1n << BigInt(width * 8 - 1);
  return value >= -limit && value < limit;
}

// ============================================================================
// As JSON
// ============================================================================

// A record's fields are saved as `name.field`, so they come back together as one object, as the core gives them.
export function appStateJson(entries: StateEntry[]): AppStateJson {
  const out: AppStateJson = {};
  for (const entry of entries) {
    const one = (value: bigint) => (entry.kind === 'bool' ? value !== 0n : Number(value));
    const value = entry.length === 0 ? (entry.values.length ? one(entry.values[0]) : null) : entry.values.map(one);
    const dot = entry.key.indexOf('.');
    if (dot < 0) {
      out[entry.key] = value;
      continue;
    }
    const record = entry.key.slice(0, dot);
    const fields = (out[record] ??= {}) as Record<string, unknown>;
    if (typeof fields === 'object' && fields !== null) {
      fields[entry.key.slice(dot + 1)] = value;
    }
  }
  return out;
}

// Takes new values from JSON shaped as `appStateJson` gives it; keys it leaves out keep their values.
// The types, lengths and widths stay as the app saved them, so the watch still loads the result.
export function applyAppStateChanges(entries: StateEntry[], changes: AppStateJson): StateEntry[] {
  return entries.map((entry) => {
    const dot = entry.key.indexOf('.');
    const change =
      dot < 0
        ? changes[entry.key]
        : (changes[entry.key.slice(0, dot)] as Record<string, unknown> | undefined)?.[entry.key.slice(dot + 1)];
    if (change === undefined) {
      return entry;
    }
    let values: bigint[];
    if (entry.length === 0) {
      values = [wholeNumber(change, entry.key)];
    } else {
      if (!Array.isArray(change) || change.length !== entry.length) {
        throw new AppStateError(`${entry.key} is a list of ${entry.length}`);
      }
      values = change.map((item) => wholeNumber(item, entry.key));
    }
    for (const value of values) {
      if (!fits(value, entry.width)) {
        throw new AppStateError(`${entry.key} cannot hold ${value}`);
      }
    }
    return { ...entry, values };
  });
}

function wholeNumber(value: unknown, key: string): bigint {
  if (typeof value === 'boolean') {
    return value ? 1n : 0n;
  }
  if (typeof value === 'number' && Number.isInteger(value)) {
    return BigInt(value);
  }
  throw new AppStateError(`${key} takes whole numbers, not ${JSON.stringify(value)}`);
}

// ============================================================================
// On the watch
// ============================================================================

export function appStatePath(id: string): string {
  return `/apps/${id}/state`;
}

// The app's state as JSON, or null when the app has never saved any.
export async function readAppStateFile(files: FileService, id: string): Promise<AppStateJson | null> {
  try {
    return appStateJson(decodeAppState(await files.read(appStatePath(id))));
  } catch (error) {
    if (error instanceof FileMissingError) {
      return null;
    }
    throw error;
  }
}

// Reads the file again, takes the changes and writes it back, as the core's sync does; the caller then tells the watch.
export async function writeAppStateFile(files: FileService, id: string, changes: AppStateJson): Promise<AppStateJson> {
  const path = appStatePath(id);
  const changed = applyAppStateChanges(decodeAppState(await files.read(path)), changes);
  await files.write(path, encodeAppState(changed));
  return appStateJson(changed);
}
