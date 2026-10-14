// The watch's qr-code app from the terminal: add a code for some text or a Wi-Fi network, and list, rename,
// move or remove the codes it keeps.

import {
  addQrCode,
  DEFAULT_QR_LEVEL,
  MOST_QR_CODES,
  moveQrCode,
  qrBitmap,
  qrDensityWarning,
  QR_LEVELS,
  readQrList,
  removeAllQrCodes,
  removeQrCode,
  renameQrCode,
  suggestedQrLabel,
  WIFI_SECURITIES,
  wifiText,
  type QrEntry,
  type QrFiles,
  type QrLevel,
  type WifiSecurity,
} from '@/protocol/qr-code';

import type { WatchSession } from '../ble';
import { say, warn } from '../output';

export const qrUsage = `neotime qr <text> [--label <label>] [--level L|M|Q|H]
neotime qr wifi <ssid> [password] [--security WPA|WEP|nopass] [--hidden] [--label <label>] [--level L|M|Q|H]
neotime qr list
neotime qr rename <number> <label>
neotime qr move <number> up|down
neotime qr remove <number>|all

  qr adds a QR code for the text, such as a link, to the end of the watch's qr-code app's list, which
  holds up to ${MOST_QR_CODES}. The label, plain ASCII of up to 24 characters, names it in the list and
  shows under the code; without one the text is used. wifi makes a code a phone's camera offers to
  join the network from, labelled with its name; the security is WPA unless given, or nopass when
  there is no password. The error correction level is ${DEFAULT_QR_LEVEL} unless given; a higher one
  survives more damage but makes a bigger code. list shows the codes numbered from 1, and rename,
  move and remove take that number; remove all takes every code off.`;

// ============================================================================
// Arguments
// ============================================================================

export type QrRequest =
  | { kind: 'add'; text: string; label: string; level: QrLevel }
  | { kind: 'list' }
  | { kind: 'rename'; number: number; label: string }
  | { kind: 'move'; number: number; step: -1 | 1 }
  | { kind: 'remove'; number: number }
  | { kind: 'remove-all' };

// A code's number in the list as the user typed it, or null when it is not one.
function listNumber(word: string | undefined): number | null {
  return word !== undefined && /^[1-9]\d*$/.test(word) ? Number(word) : null;
}

// The request for a word that manages the list, or null when the words are text to encode.
function listRequest(words: string[]): QrRequest | null {
  const [first, second, ...rest] = words;
  const number = listNumber(second);
  if (first === 'list' && words.length === 1) {
    return { kind: 'list' };
  }
  if (first === 'remove' && second === 'all' && rest.length === 0) {
    return { kind: 'remove-all' };
  }
  if (first === 'remove' && number !== null && rest.length === 0) {
    return { kind: 'remove', number };
  }
  if (first === 'rename' && number !== null && rest.length > 0) {
    return { kind: 'rename', number, label: rest.join(' ') };
  }
  if (first === 'move' && number !== null && rest.length === 1 && (rest[0] === 'up' || rest[0] === 'down')) {
    return { kind: 'move', number, step: rest[0] === 'up' ? -1 : 1 };
  }
  return null;
}

// Reads the command's arguments into a code to add, or a change to the list.
export function parseQrArguments(args: string[]): QrRequest {
  const words: string[] = [];
  let label = '';
  let level: QrLevel = DEFAULT_QR_LEVEL;
  let security: WifiSecurity | null = null;
  let hidden = false;
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    switch (arg) {
      case '--label':
        label = args[++index] ?? '';
        break;
      case '--level': {
        const given = (args[++index] ?? '').toUpperCase();
        if (!(QR_LEVELS as readonly string[]).includes(given)) {
          throw new Error(`The level is one of ${QR_LEVELS.join(', ')}, not ${given || 'nothing'}.`);
        }
        level = given as QrLevel;
        break;
      }
      case '--security': {
        const given = args[++index] ?? '';
        const match = WIFI_SECURITIES.find((option) => option.toLowerCase() === given.toLowerCase());
        if (!match) {
          throw new Error(`The security is one of ${WIFI_SECURITIES.join(', ')}, not ${given || 'nothing'}.`);
        }
        security = match;
        break;
      }
      case '--hidden':
        hidden = true;
        break;
      default:
        words.push(arg);
    }
  }

  const managing = listRequest(words);
  if (managing) {
    return managing;
  }
  const [first, ...rest] = words;
  if (first === 'wifi') {
    const [ssid, password = ''] = rest;
    if (!ssid) {
      throw new Error('Give the network name, and its password unless it has none.');
    }
    const kind = security ?? (password === '' ? 'nopass' : 'WPA');
    const text = wifiText({ ssid, password, security: kind, hidden });
    return { kind: 'add', text, label: label || suggestedQrLabel(ssid), level };
  }
  if (words.length === 0) {
    throw new Error('Give the text to put in the code, or wifi, list, rename, move or remove.');
  }
  const text = words.join(' ');
  return { kind: 'add', text, label: label || suggestedQrLabel(text), level };
}

// ============================================================================
// The command
// ============================================================================

type QrSession = Pick<WatchSession, 'files'>;

// Prints the list as numbered lines, or a note that it is empty.
function sayList(entries: QrEntry[]) {
  if (entries.length === 0) {
    say('The watch has no QR codes.');
  }
  entries.forEach((entry, place) => say(`${place + 1}. ${entry.label}`));
}

// The slot of the code with this number in the watch's list, refusing a number past its end.
async function slotOf(files: QrFiles, number: number): Promise<number> {
  const entries = await readQrList(files);
  const entry = entries[number - 1];
  if (!entry) {
    throw new Error(`The watch has ${entries.length} code${entries.length === 1 ? '' : 's'}, so no number ${number}.`);
  }
  return entry.slot;
}

export async function qr(session: QrSession, args: string[]) {
  const request = parseQrArguments(args);
  const files = await session.files();
  switch (request.kind) {
    case 'list':
      sayList(await readQrList(files));
      return;
    case 'remove-all':
      await removeAllQrCodes(files);
      say('Removed every code from the watch.');
      return;
    case 'remove':
      sayList(await removeQrCode(files, await slotOf(files, request.number)));
      return;
    case 'rename':
      sayList(await renameQrCode(files, await slotOf(files, request.number), request.label));
      return;
    case 'move':
      sayList(await moveQrCode(files, await slotOf(files, request.number), request.step));
      return;
    case 'add': {
      const bitmap = qrBitmap(request.text, request.level);
      const warning = qrDensityWarning(bitmap);
      if (warning) {
        warn(warning);
      }
      const entries = await addQrCode(files, bitmap, request.label);
      say(
        `Added a ${bitmap[0]}x${bitmap[1]} code (level ${request.level}) as number ${entries.length}, "${entries.at(-1)?.label}".`,
      );
    }
  }
}
