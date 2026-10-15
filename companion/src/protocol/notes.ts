// The note the watch's notes app shows: a text file kept at /notes/note.txt, and the font it is drawn in.
//
// docs/companion-protocol.md in the BattleTime repository describes both files. They travel over the
// file service, and each is written only when the watch does not already hold it.

import type { FileService } from '@/protocol/file-service';

// ============================================================================
// The files
// ============================================================================

export const NOTE_DIRECTORY = '/notes';
export const NOTE_PATH = `${NOTE_DIRECTORY}/note.txt`;
export const NOTE_FONT_PATH = `${NOTE_DIRECTORY}/font`;

export const NOTE_FONTS = ['small', 'normal', 'medium', 'large'] as const;
export type NoteFont = (typeof NOTE_FONTS)[number];

// A bigger note takes minutes to send, and the watch shows only about 2,000 lines of one.
export const LARGEST_NOTE_BYTES = 64 * 1024;

export class NoteTooLarge extends Error {
  constructor(bytes: number) {
    super(`The note is ${Math.ceil(bytes / 1024)} kB; the watch takes up to ${LARGEST_NOTE_BYTES / 1024} kB.`);
  }
}

// ============================================================================
// What the watch holds
// ============================================================================

// What was last sent to a watch, so an unchanged note is not sent again.
export type NoteOnWatch = { watchId: string; fingerprint: string; font: NoteFont };

// A short stand-in for the note's contents: its length and a 32-bit FNV-1a hash.
export function fingerprint(bytes: Uint8Array): string {
  let hash = 0x811c9dc5;
  for (const byte of bytes) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return `${bytes.length}:${hash.toString(16).padStart(8, '0')}`;
}

export function fontBytes(font: NoteFont): Uint8Array {
  return new TextEncoder().encode(font);
}

// ============================================================================
// Sending
// ============================================================================

export type NoteFiles = Pick<FileService, 'write' | 'list' | 'remove'>;

export type NoteSyncResult = { onWatch: NoteOnWatch; sentNote: boolean; sentFont: boolean };

// The sizes of the files in /notes on the watch, or none when the folder is missing.
async function sizesOnWatch(files: NoteFiles): Promise<Map<string, number>> {
  try {
    const entries = await files.list(NOTE_DIRECTORY);
    return new Map(entries.filter((entry) => !entry.directory).map((entry) => [entry.path, entry.size]));
  } catch {
    return new Map();
  }
}

// Writes the note and its font to the watch, each only when it differs from what the watch holds.
//
// `last` is what was sent before. With `check`, the watch's files are listed first, so a note
// deleted or replaced on the watch since is sent again.
export async function syncNote(
  files: NoteFiles,
  note: Uint8Array,
  font: NoteFont,
  watchId: string,
  last: NoteOnWatch | null,
  check: boolean,
): Promise<NoteSyncResult> {
  if (note.length > LARGEST_NOTE_BYTES) {
    throw new NoteTooLarge(note.length);
  }
  const print = fingerprint(note);
  const sameWatch = last !== null && last.watchId === watchId;
  const sizes = check ? await sizesOnWatch(files) : null;
  const noteThere =
    sameWatch && last.fingerprint === print && (sizes === null || sizes.get(NOTE_PATH) === note.length);
  const fontThere =
    sameWatch && last.font === font && (sizes === null || sizes.get(NOTE_FONT_PATH) === fontBytes(font).length);

  if (!noteThere) {
    await files.write(NOTE_PATH, note);
  }
  if (!fontThere) {
    await files.write(NOTE_FONT_PATH, fontBytes(font));
  }
  return { onWatch: { watchId, fingerprint: print, font }, sentNote: !noteThere, sentFont: !fontThere };
}

// Takes the note off the watch, so its notes app asks for one again.
export async function removeNote(files: NoteFiles): Promise<void> {
  await files.remove(NOTE_PATH);
}
