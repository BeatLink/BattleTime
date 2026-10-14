// Where the watch's note comes from: text typed in the app, a chosen file, or a URL the app fetches.
//
// Each source gives a reader that returns the note's latest bytes, or none while it holds nothing to send.

import { LARGEST_NOTE_BYTES, NoteTooLarge } from '@/protocol/notes';
import type { NoteFileAccess, NoteFileRef } from '@/services/note-file';
import { webRequester, type WebAccess, type WebRequester } from '@/services/web-request';

// How long fetching the note may take.
const FETCH_TIMEOUT_MS = 20000;

// ============================================================================
// The sources
// ============================================================================

export const NOTE_SOURCES = ['typed', 'file', 'url'] as const;
export type NoteSource = (typeof NOTE_SOURCES)[number];

// The settings each source reads.
export type NoteSourceSettings = {
  noteSource: NoteSource;
  noteText: string;
  noteFile: NoteFileRef | null;
  noteUrl: string;
  noteUrlAccess: WebAccess;
};

export type NoteReader = () => Promise<Uint8Array>;

// The reader for the chosen source, or none when that source has nothing in it yet.
export function noteReader(
  settings: NoteSourceSettings,
  access: NoteFileAccess,
  requester: WebRequester = webRequester(),
): NoteReader | null {
  switch (settings.noteSource) {
    case 'typed': {
      const text = settings.noteText;
      return text === '' ? null : async () => new TextEncoder().encode(text);
    }
    case 'file': {
      const file = settings.noteFile;
      return file === null ? null : () => access.read(file);
    }
    case 'url': {
      const url = settings.noteUrl.trim();
      return url === '' ? null : () => fetchNote(url, settings.noteUrlAccess, requester);
    }
  }
}

// What the chosen source is, in a few words, for the screen.
export function describeSource(settings: NoteSourceSettings): string | null {
  switch (settings.noteSource) {
    case 'typed':
      return settings.noteText === '' ? null : 'The note typed here';
    case 'file':
      return settings.noteFile?.name ?? null;
    case 'url':
      return settings.noteUrl.trim() || null;
  }
}

// ============================================================================
// Fetching from a URL
// ============================================================================

// Fetches the note's text from an http or https URL with its headers, login and certificate choice,
// refusing an error reply or one too big for the watch.
export async function fetchNote(
  url: string,
  access: WebAccess = {},
  requester: WebRequester = webRequester(),
): Promise<Uint8Array> {
  if (!/^https?:\/\//i.test(url)) {
    throw new Error(`${url} is not an http or https URL.`);
  }
  let reply;
  try {
    reply = await requester({
      method: 'GET',
      url,
      headers: { Accept: 'text/plain, */*' },
      access,
      timeoutMs: FETCH_TIMEOUT_MS,
    });
  } catch (error) {
    throw new Error(`Could not fetch ${url}: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (reply.status < 200 || reply.status > 299) {
    throw new Error(`${url} answered ${reply.status}${reply.statusText ? ` ${reply.statusText}` : ''}.`);
  }
  if (reply.body.length > LARGEST_NOTE_BYTES) {
    throw new NoteTooLarge(reply.body.length);
  }
  return reply.body;
}
