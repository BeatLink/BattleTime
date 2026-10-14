// The codes the watch's qr-code app lists: each a bitmap at /qr/<slot>, and their order and captions in /qr/list.
//
// docs/companion-protocol.md in the NeoTime repository describes the files. They travel over the file
// service; adding a code writes its bitmap and the list, and renaming, moving or removing one rewrites
// only the list.

import { create } from 'qrcode/lib/core/qrcode';

import { FileMissingError, type FileService } from '@/protocol/file-service';

// ============================================================================
// The files
// ============================================================================

export const QR_DIRECTORY = '/qr';
export const QR_LIST_PATH = `${QR_DIRECTORY}/list`;
export const QR_LIST_VERSION = 1;

// The most codes the watch keeps, each in its own slot from 0.
export const MOST_QR_CODES = 8;

// The file holding the code in a slot.
export function qrCodePath(slot: number): string {
  return `${QR_DIRECTORY}/${slot}`;
}

// The watch refuses a code wider than this many modules.
export const LARGEST_QR_MODULES = 120;

// Above this width a module is under 3 pixels on the 240 pixel screen and may not scan.
export const LARGEST_EASY_QR_MODULES = 74;

// The longest caption the watch shows under the code.
export const LARGEST_QR_LABEL_BYTES = 24;

export const QR_LEVELS = ['L', 'M', 'Q', 'H'] as const;
export type QrLevel = (typeof QR_LEVELS)[number];
export const DEFAULT_QR_LEVEL: QrLevel = 'M';

export class QrCodeError extends Error {}

// ============================================================================
// Encoding
// ============================================================================

// A QR code's modules, row by row, one byte a module, 1 for dark.
export type QrModules = { size: number; data: Uint8Array };

// The modules of the QR code for the text, with no quiet zone.
export function qrModules(text: string, level: QrLevel = DEFAULT_QR_LEVEL): QrModules {
  if (text === '') {
    throw new QrCodeError('Give the text to put in the code.');
  }
  try {
    const { modules } = create(text, { errorCorrectionLevel: level });
    return { size: modules.size, data: modules.data };
  } catch (error) {
    throw new QrCodeError(`The text is too long for a QR code: ${(error as Error).message}`);
  }
}

// The /qr/code file for the modules: width, height, then each row packed most significant bit first.
export function packQrModules(modules: QrModules): Uint8Array {
  const { size, data } = modules;
  if (size < 1 || size > LARGEST_QR_MODULES) {
    throw new QrCodeError(`The code is ${size} modules wide; the watch takes up to ${LARGEST_QR_MODULES}.`);
  }
  const rowBytes = Math.ceil(size / 8);
  const bitmap = new Uint8Array(2 + size * rowBytes);
  bitmap[0] = size;
  bitmap[1] = size;
  for (let row = 0; row < size; row++) {
    for (let column = 0; column < size; column++) {
      if (data[row * size + column]) {
        bitmap[2 + row * rowBytes + (column >> 3)] |= 0x80 >> (column & 7);
      }
    }
  }
  return bitmap;
}

// Whether a module at the row and column of a packed bitmap is dark.
export function bitmapModule(bitmap: Uint8Array, row: number, column: number): boolean {
  const rowBytes = Math.ceil(bitmap[0] / 8);
  return (bitmap[2 + row * rowBytes + (column >> 3)] & (0x80 >> (column & 7))) !== 0;
}

// A stretch of dark modules side by side in one row of a packed bitmap.
export type DarkRun = { row: number; column: number; length: number };

// The dark modules of a packed bitmap as runs along each row, so a preview draws one box a run.
export function darkRuns(bitmap: Uint8Array): DarkRun[] {
  const [width, height] = bitmap;
  const runs: DarkRun[] = [];
  for (let row = 0; row < height; row++) {
    let start = -1;
    for (let column = 0; column <= width; column++) {
      const dark = column < width && bitmapModule(bitmap, row, column);
      if (dark && start < 0) {
        start = column;
      } else if (!dark && start >= 0) {
        runs.push({ row, column: start, length: column - start });
        start = -1;
      }
    }
  }
  return runs;
}

// The /qr/code file for the text, refusing a code too wide for the watch.
export function qrBitmap(text: string, level: QrLevel = DEFAULT_QR_LEVEL): Uint8Array {
  return packQrModules(qrModules(text, level));
}

// A warning when the code is so dense that it may not scan from the watch, or null when it is fine.
export function qrDensityWarning(bitmap: Uint8Array): string | null {
  const size = bitmap[0];
  if (size <= LARGEST_EASY_QR_MODULES) {
    return null;
  }
  return `The code is ${size} modules wide, so each is under 3 pixels on the watch and it may not scan. Shorter text or a lower error correction level makes it smaller.`;
}

// ============================================================================
// The caption
// ============================================================================

// The caption as the watch can show it: printable ASCII, accents dropped, at most 24 bytes.
export function cleanQrLabel(label: string): string {
  return label
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[\r\n\t]+/g, ' ')
    .replace(/[^\x20-\x7e]/gu, '?')
    .trim()
    .slice(0, LARGEST_QR_LABEL_BYTES)
    .trimEnd();
}
// A caption for a code given none: the text itself without a link's scheme, cut to fit.
export function suggestedQrLabel(text: string): string {
  return cleanQrLabel(text.replace(/^[a-z][a-z0-9+.-]*:\/\//i, ''));
}

// ============================================================================
// Wi-Fi codes
// ============================================================================

export const WIFI_SECURITIES = ['WPA', 'WEP', 'nopass'] as const;
export type WifiSecurity = (typeof WIFI_SECURITIES)[number];

export type WifiNetwork = { ssid: string; password: string; security: WifiSecurity; hidden: boolean };

// Escapes the characters the Wi-Fi code format gives a meaning to.
export function escapeWifiField(value: string): string {
  return value.replace(/([\\;,:"])/g, '\\$1');
}

// The text a phone's camera reads as an offer to join the network.
export function wifiText(network: WifiNetwork): string {
  if (network.ssid === '') {
    throw new QrCodeError('Give the network name.');
  }
  const password = network.security === 'nopass' ? '' : escapeWifiField(network.password);
  return `WIFI:T:${network.security};S:${escapeWifiField(network.ssid)};P:${password};H:${network.hidden};;`;
}
// ============================================================================
// The list
// ============================================================================

// One code on the watch: the slot its file is in, and the caption the watch lists it by.
export type QrEntry = { slot: number; label: string };

// Each entry is its slot, its caption's length, then the caption padded with zeros to full length.
const QR_ENTRY_BYTES = 2 + LARGEST_QR_LABEL_BYTES;

// The /qr/list file for the entries, in the order the watch lists them.
export function packQrList(entries: QrEntry[]): Uint8Array {
  if (entries.length > MOST_QR_CODES) {
    throw new QrCodeError(`The watch keeps up to ${MOST_QR_CODES} codes.`);
  }
  const file = new Uint8Array(2 + entries.length * QR_ENTRY_BYTES);
  file[0] = QR_LIST_VERSION;
  file[1] = entries.length;
  entries.forEach((entry, place) => {
    const caption = new TextEncoder().encode(cleanQrLabel(entry.label));
    const start = 2 + place * QR_ENTRY_BYTES;
    file[start] = entry.slot;
    file[start + 1] = caption.length;
    file.set(caption, start + 2);
  });
  return file;
}

// The entries of a /qr/list file, or none for a file in another format.
export function parseQrList(file: Uint8Array): QrEntry[] {
  if (file[0] !== QR_LIST_VERSION) {
    return [];
  }
  const count = Math.min(file[1] ?? 0, MOST_QR_CODES);
  const entries: QrEntry[] = [];
  for (let place = 0; place < count; place++) {
    const start = 2 + place * QR_ENTRY_BYTES;
    if (start + QR_ENTRY_BYTES > file.length) {
      break;
    }
    const length = Math.min(file[start + 1], LARGEST_QR_LABEL_BYTES);
    const label = new TextDecoder().decode(file.subarray(start + 2, start + 2 + length));
    entries.push({ slot: file[start], label });
  }
  return entries;
}

// ============================================================================
// Changing the codes on the watch
// ============================================================================

export type QrFiles = Pick<FileService, 'read' | 'write' | 'remove'>;

async function removeIfThere(files: QrFiles, path: string): Promise<void> {
  try {
    await files.remove(path);
  } catch (error) {
    if (!(error instanceof FileMissingError)) {
      throw error;
    }
  }
}

// The codes on the watch in the order it lists them, or none when it has no list.
export async function readQrList(files: QrFiles): Promise<QrEntry[]> {
  try {
    return parseQrList(await files.read(QR_LIST_PATH));
  } catch (error) {
    if (error instanceof FileMissingError) {
      return [];
    }
    throw error;
  }
}

async function writeQrList(files: QrFiles, entries: QrEntry[]): Promise<QrEntry[]> {
  await files.write(QR_LIST_PATH, packQrList(entries));
  return entries;
}

// A caption the watch can list, refusing one with nothing left to show.
function listedLabel(label: string): string {
  const caption = cleanQrLabel(label);
  if (caption === '') {
    throw new QrCodeError('Give the code a caption, so the watch can list it.');
  }
  return caption;
}

// Adds a code at the end of the watch's list, in the first free slot; gives the new list.
export async function addQrCode(files: QrFiles, bitmap: Uint8Array, label: string): Promise<QrEntry[]> {
  const caption = listedLabel(label);
  const entries = await readQrList(files);
  const used = new Set(entries.map((entry) => entry.slot));
  const slot = [...Array(MOST_QR_CODES).keys()].find((candidate) => !used.has(candidate));
  if (slot === undefined) {
    throw new QrCodeError(`The watch already has ${MOST_QR_CODES} codes; remove one first.`);
  }
  // The code goes first, so the list never names a slot whose file is not there yet.
  await files.write(qrCodePath(slot), bitmap);
  return writeQrList(files, [...entries, { slot, label: caption }]);
}

// Gives the code in a slot a new caption; gives the new list.
export async function renameQrCode(files: QrFiles, slot: number, label: string): Promise<QrEntry[]> {
  const caption = listedLabel(label);
  const entries = await readQrList(files);
  return writeQrList(
    files,
    entries.map((entry) => (entry.slot === slot ? { slot, label: caption } : entry)),
  );
}

// Moves the code in a slot up (-1) or down (+1) the watch's list; gives the new list.
export async function moveQrCode(files: QrFiles, slot: number, step: -1 | 1): Promise<QrEntry[]> {
  const entries = await readQrList(files);
  const place = entries.findIndex((entry) => entry.slot === slot);
  const other = place + step;
  if (place < 0 || other < 0 || other >= entries.length) {
    return entries;
  }
  const moved = [...entries];
  [moved[place], moved[other]] = [moved[other], moved[place]];
  return writeQrList(files, moved);
}

// Takes the code in a slot off the watch; gives the new list.
export async function removeQrCode(files: QrFiles, slot: number): Promise<QrEntry[]> {
  const entries = await readQrList(files);
  // The list goes first, so it never names a slot whose file is gone.
  const left = await writeQrList(
    files,
    entries.filter((entry) => entry.slot !== slot),
  );
  await removeIfThere(files, qrCodePath(slot));
  return left;
}

// Takes every code off the watch, so its app shows its hint again.
export async function removeAllQrCodes(files: QrFiles): Promise<void> {
  await removeIfThere(files, QR_LIST_PATH);
  for (let slot = 0; slot < MOST_QR_CODES; slot++) {
    await removeIfThere(files, qrCodePath(slot));
  }
}
