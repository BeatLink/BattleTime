import { describe, expect, it } from '@jest/globals';

import { LARGEST_NOTE_BYTES } from '@/protocol/notes';
import type { NoteFileAccess } from '@/services/note-file';
import { fetchRequester, type WebRequest } from '@/services/web-request';

import { describeSource, fetchNote, noteReader, type NoteSourceSettings } from './note-source';

// ============================================================================
// A file, a web server and settings, all in memory
// ============================================================================

const access: NoteFileAccess = {
  pick: async () => null,
  read: async (file) => new TextEncoder().encode(`contents of ${file.name}`),
  watch: () => null,
};

const empty: NoteSourceSettings = { noteSource: 'typed', noteText: '', noteFile: null, noteUrl: '', noteUrlAccess: {} };

// A fetch that answers every URL with the same reply, and remembers what it was asked for.
function server(body: string, status = 200) {
  const asked: WebRequest[] = [];
  const fetcher = (async (request: WebRequest) => {
    asked.push(request);
    return { status, statusText: status === 200 ? 'OK' : 'Not Found', body: new TextEncoder().encode(body) };
  });
  return { fetcher, asked };
}

const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

// ============================================================================
// Readers
// ============================================================================

describe('noteReader', () => {
  it('reads typed text, a chosen file or a URL, whichever is the source', async () => {
    const { fetcher, asked } = server('From the web');
    const settings: NoteSourceSettings = {
      noteSource: 'typed',
      noteText: 'Milk',
      noteFile: { name: 'list.txt', uri: '/home/me/list.txt' },
      noteUrl: ' https://example.org/note.txt ',
      noteUrlAccess: { username: 'me', password: 'secret', certificate: 'any' },
    };

    expect(text(await noteReader(settings, access, fetcher)!())).toBe('Milk');
    expect(text(await noteReader({ ...settings, noteSource: 'file' }, access, fetcher)!())).toBe(
      'contents of list.txt',
    );
    expect(text(await noteReader({ ...settings, noteSource: 'url' }, access, fetcher)!())).toBe('From the web');
    expect(asked).toMatchObject([{ method: 'GET', url: 'https://example.org/note.txt', access: settings.noteUrlAccess }]);
  });

  it('gives no reader while the chosen source is empty', () => {
    expect(noteReader(empty, access)).toBeNull();
    expect(noteReader({ ...empty, noteSource: 'file' }, access)).toBeNull();
    expect(noteReader({ ...empty, noteSource: 'url', noteUrl: '  ' }, access)).toBeNull();
  });

  it('names the chosen source for the screen', () => {
    expect(describeSource(empty)).toBeNull();
    expect(describeSource({ ...empty, noteText: 'Milk' })).toBe('The note typed here');
    expect(describeSource({ ...empty, noteSource: 'url', noteUrl: 'https://example.org/n' })).toBe(
      'https://example.org/n',
    );
  });
});

describe('fetchNote', () => {
  it('refuses a URL that is not http or https', async () => {
    await expect(fetchNote('file:///etc/passwd')).rejects.toThrow('is not an http or https URL');
  });

  it('cannot trust a self-signed certificate through fetch alone', async () => {
    const fetcher = fetchRequester((async () => new Response('x')) as unknown as typeof fetch);
    await expect(fetchNote('https://nas.local/n', { certificate: 'any' }, fetcher)).rejects.toThrow(
      'Trusting a self-signed certificate needs the desktop app or the Android app.',
    );
  });

  it('reports an error reply, a failed fetch and a note too big for the watch', async () => {
    await expect(fetchNote('https://example.org/gone', {}, server('', 404).fetcher)).rejects.toThrow(
      'https://example.org/gone answered 404 Not Found.',
    );
    const offline = async () => {
      throw new Error('fetch failed');
    };
    await expect(fetchNote('https://example.org/n', {}, offline)).rejects.toThrow(
      'Could not fetch https://example.org/n: fetch failed',
    );
    const huge = 'x'.repeat(LARGEST_NOTE_BYTES + 1);
    await expect(fetchNote('https://example.org/n', {}, server(huge).fetcher)).rejects.toThrow('the watch takes up to 64 kB');
  });
});
