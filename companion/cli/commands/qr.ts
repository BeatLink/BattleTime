// The watch's qr-code app from the terminal: send it a code for some text or a Wi-Fi network, or take it off.

import {
  cleanQrLabel,
  DEFAULT_QR_LEVEL,
  qrBitmap,
  qrDensityWarning,
  QR_CODE_PATH,
  QR_LEVELS,
  removeQrCode,
  sendQrCode,
  WIFI_SECURITIES,
  wifiText,
  type QrLevel,
  type WifiSecurity,
} from '@/protocol/qr-code';

import type { WatchSession } from '../ble';
import { say, warn } from '../output';

export const qrUsage = `neotime qr <text> [--label <label>] [--level L|M|Q|H]
neotime qr wifi <ssid> [password] [--security WPA|WEP|nopass] [--hidden] [--label <label>] [--level L|M|Q|H]
neotime qr remove

  qr sends a QR code for the text, such as a link, to the watch's qr-code app, with an optional
  caption under it in plain ASCII of up to 24 characters. wifi makes a code a phone's camera offers
  to join the network from; the security is WPA unless given, or nopass when there is no password.
  The error correction level is ${DEFAULT_QR_LEVEL} unless given; a higher one survives more damage
  but makes a bigger code. remove takes the code off the watch.`;

// ============================================================================
// Arguments
// ============================================================================

export type QrRequest =
  | { kind: 'remove' }
  | { kind: 'send'; text: string; label: string; level: QrLevel };

// Reads the command's arguments into the text to encode, its caption and level, or a removal.
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

  const [first, ...rest] = words;
  if (first === 'remove' && rest.length === 0) {
    return { kind: 'remove' };
  }
  if (first === 'wifi') {
    const [ssid, password = ''] = rest;
    if (!ssid) {
      throw new Error('Give the network name, and its password unless it has none.');
    }
    const kind = security ?? (password === '' ? 'nopass' : 'WPA');
    return { kind: 'send', text: wifiText({ ssid, password, security: kind, hidden }), label, level };
  }
  if (words.length === 0) {
    throw new Error('Give the text to put in the code, or wifi, or remove.');
  }
  return { kind: 'send', text: words.join(' '), label, level };
}

// ============================================================================
// The command
// ============================================================================

type QrSession = Pick<WatchSession, 'files'>;

export async function qr(session: QrSession, args: string[]) {
  const request = parseQrArguments(args);
  const files = await session.files();
  if (request.kind === 'remove') {
    await removeQrCode(files);
    say('Removed the code from the watch.');
    return;
  }
  const bitmap = qrBitmap(request.text, request.level);
  const warning = qrDensityWarning(bitmap);
  if (warning) {
    warn(warning);
  }
  await sendQrCode(files, bitmap, request.label);
  const caption = cleanQrLabel(request.label);
  say(
    `Sent a ${bitmap[0]}x${bitmap[1]} code (level ${request.level}) to ${QR_CODE_PATH}${caption ? `, captioned "${caption}"` : ''}.`,
  );
}
