// The watch face's quick-app ring: the apps a swipe left or right on the face steps through.
//
// The watch reads the ring from /settings/quick_apps each time the wearer steps. The file is
// UTF-8 text with one app id a line, in ring order; the watch ignores blank lines, spaces and
// carriage returns, and skips ids that are not installed. Without the file it uses DEFAULT_QUICK_APPS.

import { FileMissingError, type FileService } from '@/protocol/file-service';

// ============================================================================
// The file
// ============================================================================

export const QUICK_APPS_PATH = '/settings/quick_apps';

// The ring the watch uses when the file is missing.
export const DEFAULT_QUICK_APPS: readonly string[] = ['steps', 'stopwatch', 'heart', 'weather', 'music'];

export class QuickAppsError extends Error {}

// The file's text for a ring: one id a line, each line ending in a newline.
export function encodeQuickApps(ids: readonly string[]): Uint8Array {
  for (const id of ids) {
    if (id === '' || /\s/.test(id)) {
      throw new QuickAppsError(`"${id}" is not an app id; an id has no spaces or line breaks.`);
    }
  }
  return new TextEncoder().encode(ids.map((id) => `${id}\n`).join(''));
}

// The ids in the file's text, in order, as the watch reads them.
export function decodeQuickApps(bytes: Uint8Array): string[] {
  return new TextDecoder()
    .decode(bytes)
    .split('\n')
    .map((line) => line.replace(/\r/g, '').trim())
    .filter((line) => line !== '');
}

// ============================================================================
// Reading and writing the watch's ring
// ============================================================================

export type QuickAppsFiles = Pick<FileService, 'read' | 'write' | 'remove'>;

// The ring on the watch, and whether it came from the file rather than the watch's default.
export type QuickAppsOnWatch = { ids: string[]; fromFile: boolean };

// Reads the watch's ring, falling back to the default when the file is missing.
export async function readQuickApps(files: QuickAppsFiles): Promise<QuickAppsOnWatch> {
  try {
    return { ids: decodeQuickApps(await files.read(QUICK_APPS_PATH)), fromFile: true };
  } catch (error) {
    if (error instanceof FileMissingError) {
      return { ids: [...DEFAULT_QUICK_APPS], fromFile: false };
    }
    throw error;
  }
}

// Writes the ring to the watch, replacing what it held.
export async function writeQuickApps(files: QuickAppsFiles, ids: readonly string[]): Promise<void> {
  await files.write(QUICK_APPS_PATH, encodeQuickApps(ids));
}

// Deletes the file so the watch goes back to its default ring; a file already gone is fine.
export async function resetQuickApps(files: QuickAppsFiles): Promise<void> {
  try {
    await files.remove(QUICK_APPS_PATH);
  } catch (error) {
    if (!(error instanceof FileMissingError)) {
      throw error;
    }
  }
}
