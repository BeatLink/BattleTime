import { describe, expect, it } from '@jest/globals';

import { LARGEST_NOTE_BYTES } from '@/protocol/notes';
import type { NoteFileAccess } from '@/services/note-file';

import { describeSource, fetchNote, noteReader, type NoteSourceSettings } from './note-source';

// ============================================================================
// A file, a web server and settings, all in memory
// ============================================================================

const access: NoteFileAccess = {
  pick: async () => null,
  read: async (file) => new TextEncoder().encode(`contents of ${file.name}`),
  watch: () => null,
};

const empty: NoteSourceSettings = { noteSource: 'typed', noteText: '', noteFile: null, noteUrl: '' };

// A fetch that answers every URL with the same reply, and remembers what it was asked for.
function server(body: string, status = 200) {
  const asked: string[] = [];
  const fetcher = (async (url: string) => {
    asked.push(url);
    return new Response(body, { status, statusText: status === 200 ? 'OK' : 'Not Found' });
  }) as unknown as typeof fetch;
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
    };

    expect(text(await noteReader(settings, access, fetcher)!())).toBe('Milk');
    expect(text(await noteReader({ ...settings, noteSource: 'file' }, access, fetcher)!())).toBe(
      'contents of list.txt',
    );
    expect(text(await noteReader({ ...settings, noteSource: 'url' }, access, fetcher)!())).toBe('From the web');
    expect(asked).toEqual(['https://example.org/note.txt']);
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

  it('reports an error reply, a failed fetch and a note too big for the watch', async () => {
    await expect(fetchNote('https://example.org/gone', server('', 404).fetcher)).rejects.toThrow(
      'https://example.org/gone answered 404 Not Found.',
    );
    const offline = (async () => {
      throw new Error('fetch failed');
    }) as unknown as typeof fetch;
    await expect(fetchNote('https://example.org/n', offline)).rejects.toThrow(
      'Could not fetch https://example.org/n: fetch failed',
    );
    const huge = 'x'.repeat(LARGEST_NOTE_BYTES + 1);
    await expect(fetchNote('https://example.org/n', server(huge).fetcher)).rejects.toThrow('the watch takes up to 64 kB');
  });
});
