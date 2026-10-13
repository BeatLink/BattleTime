// The watch's remote app: up to nine buttons the user sets up here, which the watch shows and reports taps on.
//
// The watch only holds each button's icon, in the file /remote/buttons, and sends
// {"t":"app","ch":"remote","d":{"button":<index>}} when one is tapped. What a button does lives only in the
// companion's settings, so a tap can never carry a command, a URL or an intent from the watch.

import { LIBRARY_ICON_NAMES, type LibraryIconName } from '@/bundles/icon-names';
import type { FileService } from '@/protocol/file-service';
import type { WatchToPhoneMessage } from '@/protocol/gadgetbridge';
import { fingerprint } from '@/protocol/notes';

// ============================================================================
// Buttons and their actions
// ============================================================================

export const MOST_REMOTE_BUTTONS = 9;

export const HTTP_METHODS = ['GET', 'POST', 'PUT', 'DELETE'] as const;
export type HttpMethod = (typeof HTTP_METHODS)[number];

// Ready-made actions the companion already knows how to do.
export const BUILTIN_ACTIONS = [
  'playpause',
  'play',
  'pause',
  'next',
  'previous',
  'volumeup',
  'volumedown',
  'findphone',
] as const;
export type BuiltinAction = (typeof BUILTIN_ACTIONS)[number];

export type AndroidIntent =
  | { type: 'deeplink'; uri: string }
  | { type: 'activity'; packageName: string; className: string; action?: string; extras?: Record<string, string> }
  | { type: 'broadcast'; action: string; packageName?: string; extras?: Record<string, string> };

export type RemoteAction =
  | { kind: 'http'; method: HttpMethod; url: string; headers?: Record<string, string>; body?: string }
  | { kind: 'command'; command: string }
  | { kind: 'android'; intent: AndroidIntent }
  | { kind: 'builtin'; name: BuiltinAction };

export type RemoteActionKind = RemoteAction['kind'];

// One button: the library icon the watch shows, a label only the companion shows, and what a tap does.
export type RemoteButton = { icon: LibraryIconName; label: string; action: RemoteAction };

// ============================================================================
// Reading buttons back from storage or a file
// ============================================================================

const isText = (value: unknown): value is string => typeof value === 'string';

function readExtras(value: unknown): Record<string, string> | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const entries = Object.entries(value).filter(([, item]) => isText(item)) as [string, string][];
  return entries.length ? Object.fromEntries(entries) : undefined;
}

function readIntent(value: Record<string, unknown>): AndroidIntent | null {
  switch (value.type) {
    case 'deeplink':
      return isText(value.uri) ? { type: 'deeplink', uri: value.uri } : null;
    case 'activity':
      return isText(value.packageName) && isText(value.className)
        ? {
            type: 'activity',
            packageName: value.packageName,
            className: value.className,
            ...(isText(value.action) && value.action ? { action: value.action } : {}),
            ...(readExtras(value.extras) ? { extras: readExtras(value.extras) } : {}),
          }
        : null;
    case 'broadcast':
      return isText(value.action)
        ? {
            type: 'broadcast',
            action: value.action,
            ...(isText(value.packageName) && value.packageName ? { packageName: value.packageName } : {}),
            ...(readExtras(value.extras) ? { extras: readExtras(value.extras) } : {}),
          }
        : null;
    default:
      return null;
  }
}

// One action as stored, or null when it is not one this app knows.
export function readRemoteAction(value: unknown): RemoteAction | null {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const action = value as Record<string, unknown>;
  switch (action.kind) {
    case 'http':
      return (HTTP_METHODS as readonly unknown[]).includes(action.method) && isText(action.url)
        ? {
            kind: 'http',
            method: action.method as HttpMethod,
            url: action.url,
            ...(readExtras(action.headers) ? { headers: readExtras(action.headers) } : {}),
            ...(isText(action.body) && action.body ? { body: action.body } : {}),
          }
        : null;
    case 'command':
      return isText(action.command) ? { kind: 'command', command: action.command } : null;
    case 'android': {
      const intent = action.intent && typeof action.intent === 'object' ? readIntent(action.intent as Record<string, unknown>) : null;
      return intent ? { kind: 'android', intent } : null;
    }
    case 'builtin':
      return (BUILTIN_ACTIONS as readonly unknown[]).includes(action.name)
        ? { kind: 'builtin', name: action.name as BuiltinAction }
        : null;
    default:
      return null;
  }
}

// The buttons in a stored list, dropping any that are malformed and any past the ninth.
export function readRemoteButtons(value: unknown): RemoteButton[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const buttons: RemoteButton[] = [];
  for (const item of value) {
    if (!item || typeof item !== 'object') continue;
    const { icon, label, action } = item as Record<string, unknown>;
    const known = readRemoteAction(action);
    if (!isLibraryIcon(icon) || !known) continue;
    buttons.push({ icon, label: isText(label) ? label : '', action: known });
  }
  return buttons.slice(0, MOST_REMOTE_BUTTONS);
}

export function isLibraryIcon(name: unknown): name is LibraryIconName {
  return (LIBRARY_ICON_NAMES as readonly unknown[]).includes(name);
}

// ============================================================================
// The layout file
// ============================================================================

export const REMOTE_DIRECTORY = '/remote';
export const REMOTE_BUTTONS_PATH = `${REMOTE_DIRECTORY}/buttons`;
export const REMOTE_LAYOUT_VERSION = 1;

export class RemoteLayoutError extends Error {}

// Byte 0 is the format version, byte 1 the button count, then each button's icon as its place in the library.
export function encodeRemoteLayout(icons: readonly string[]): Uint8Array {
  if (icons.length > MOST_REMOTE_BUTTONS) {
    throw new RemoteLayoutError(`The watch shows up to ${MOST_REMOTE_BUTTONS} buttons, not ${icons.length}.`);
  }
  const bytes = new Uint8Array(2 + icons.length);
  bytes[0] = REMOTE_LAYOUT_VERSION;
  bytes[1] = icons.length;
  icons.forEach((icon, index) => {
    const number = (LIBRARY_ICON_NAMES as readonly string[]).indexOf(icon);
    if (number < 0) {
      throw new RemoteLayoutError(`The shared library has no icon called ${icon}.`);
    }
    bytes[2 + index] = number;
  });
  return bytes;
}

// The icon names in a layout file, for checking what the watch holds.
export function decodeRemoteLayout(bytes: Uint8Array): LibraryIconName[] {
  if (bytes.length < 2 || bytes[0] !== REMOTE_LAYOUT_VERSION) {
    throw new RemoteLayoutError('That is not a version 1 remote layout.');
  }
  const count = bytes[1];
  if (count > MOST_REMOTE_BUTTONS || bytes.length !== 2 + count) {
    throw new RemoteLayoutError('The remote layout is the wrong length for its button count.');
  }
  return Array.from(bytes.subarray(2), (number) => {
    const name = LIBRARY_ICON_NAMES[number];
    if (!name) {
      throw new RemoteLayoutError(`The remote layout names icon ${number}, which the library does not have.`);
    }
    return name;
  });
}

// ============================================================================
// Sending the layout
// ============================================================================

// What was last sent to a watch, so an unchanged layout is not sent again.
export type RemoteOnWatch = { watchId: string; fingerprint: string };

export type RemoteFiles = Pick<FileService, 'write' | 'list'>;

// Writes the layout unless the watch already holds it; with `check`, the watch's folder is listed first.
export async function syncRemoteLayout(
  files: RemoteFiles,
  buttons: readonly RemoteButton[],
  watchId: string,
  last: RemoteOnWatch | null,
  check: boolean,
): Promise<{ onWatch: RemoteOnWatch; sent: boolean }> {
  const layout = encodeRemoteLayout(buttons.map((button) => button.icon));
  const print = fingerprint(layout);
  let there = last !== null && last.watchId === watchId && last.fingerprint === print;
  if (there && check) {
    try {
      const entries = await files.list(REMOTE_DIRECTORY);
      there = entries.some((entry) => entry.path === REMOTE_BUTTONS_PATH && entry.size === layout.length);
    } catch {
      there = false;
    }
  }
  if (!there) {
    await files.write(REMOTE_BUTTONS_PATH, layout);
  }
  return { onWatch: { watchId, fingerprint: print }, sent: !there };
}

// ============================================================================
// Taps from the watch
// ============================================================================

export const REMOTE_CHANNEL = 'remote';

// The index of the tapped button, or null when the message is not a well-formed remote tap.
export function remoteTapIndex(message: WatchToPhoneMessage): number | null {
  if (message.t !== 'app' || message.ch !== REMOTE_CHANNEL) {
    return null;
  }
  const data = message.d;
  if (!data || typeof data !== 'object') {
    return null;
  }
  const button = (data as { button?: unknown }).button;
  return typeof button === 'number' && Number.isInteger(button) && button >= 0 ? button : null;
}
