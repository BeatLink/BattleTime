// The code the watch's qr-code app shows: a bitmap at /qr/code and an optional caption at /qr/label.
//
// docs/companion-protocol.md in the NeoTime repository describes both files. They travel over the
// file service and are written whole each time the wearer sends a new code.

import { create } from 'qrcode/lib/core/qrcode';

import { FileMissingError, type FileService } from '@/protocol/file-service';

// ============================================================================
// The files
// ============================================================================

export const QR_DIRECTORY = '/qr';
export const QR_CODE_PATH = `${QR_DIRECTORY}/code`;
export const QR_LABEL_PATH = `${QR_DIRECTORY}/label`;

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
// Sending
// ============================================================================

export type QrFiles = Pick<FileService, 'write' | 'remove'>;

// Writes the code and its caption to the watch, deleting the caption file when there is none.
export async function sendQrCode(files: QrFiles, bitmap: Uint8Array, label: string): Promise<void> {
  await files.write(QR_CODE_PATH, bitmap);
  const caption = cleanQrLabel(label);
  if (caption !== '') {
    await files.write(QR_LABEL_PATH, new TextEncoder().encode(caption));
    return;
  }
  try {
    await files.remove(QR_LABEL_PATH);
  } catch (error) {
    if (!(error instanceof FileMissingError)) {
      throw error;
    }
  }
}

// Takes the code and its caption off the watch, so its app shows its hint again.
export async function removeQrCode(files: QrFiles): Promise<void> {
  for (const path of [QR_CODE_PATH, QR_LABEL_PATH]) {
    try {
      await files.remove(path);
    } catch (error) {
      if (!(error instanceof FileMissingError)) {
        throw error;
      }
    }
  }
}
