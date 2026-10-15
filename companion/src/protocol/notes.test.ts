import { describe, expect, it } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { FileService, type WatchFile } from './file-service';
import {
  fingerprint,
  LARGEST_NOTE_BYTES,
  NOTE_FONT_PATH,
  NOTE_PATH,
  NoteTooLarge,
  removeNote,
  syncNote,
  type NoteFiles,
} from './notes';

// ============================================================================
// A watch's files in memory
// ============================================================================

class FakeFiles implements NoteFiles {
  readonly files = new Map<string, Uint8Array>();
  readonly writes: string[] = [];

  async write(path: string, contents: Uint8Array): Promise<void> {
    this.writes.push(path);
    this.files.set(path, contents);
  }

  async list(dir: string): Promise<WatchFile[]> {
    const inside = [...this.files].filter(([path]) => path.startsWith(`${dir}/`));
    if (inside.length === 0) {
      throw new Error(`${dir}: no such folder`);
    }
    return inside.map(([path, data]) => ({ name: path.slice(dir.length + 1), path, size: data.length, directory: false }));
  }

  async remove(path: string): Promise<void> {
    this.files.delete(path);
  }
}

const encode = (text: string) => new TextEncoder().encode(text);
const decode = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);

// ============================================================================
// Fingerprints
// ============================================================================

describe('fingerprint', () => {
  it('changes with the contents and the length', () => {
    expect(fingerprint(encode('abc'))).toBe(fingerprint(encode('abc')));
    expect(fingerprint(encode('abc'))).not.toBe(fingerprint(encode('abd')));
    expect(fingerprint(encode('abc'))).toMatch(/^3:[0-9a-f]{8}$/);
    expect(fingerprint(new Uint8Array(0))).toBe('0:811c9dc5');
  });
});

// ============================================================================
// Sending
// ============================================================================

describe('syncNote', () => {
  it('sends the note and its font to a watch that has neither', async () => {
    const files = new FakeFiles();

    const result = await syncNote(files, encode('Milk\nBread\n'), 'large', 'watch-1', null, true);

    expect(decode(files.files.get(NOTE_PATH))).toBe('Milk\nBread\n');
    expect(decode(files.files.get(NOTE_FONT_PATH))).toBe('large');
    expect(result).toEqual({
      onWatch: { watchId: 'watch-1', fingerprint: fingerprint(encode('Milk\nBread\n')), font: 'large' },
      sentNote: true,
      sentFont: true,
    });
  });

  it('sends nothing when the watch already holds the same note and font', async () => {
    const files = new FakeFiles();
    const first = await syncNote(files, encode('Milk'), 'small', 'watch-1', null, true);
    files.writes.length = 0;

    const checked = await syncNote(files, encode('Milk'), 'small', 'watch-1', first.onWatch, true);
    const trusted = await syncNote(files, encode('Milk'), 'small', 'watch-1', first.onWatch, false);

    expect(files.writes).toEqual([]);
    expect([checked.sentNote, checked.sentFont, trusted.sentNote, trusted.sentFont]).toEqual([false, false, false, false]);
  });

  it('sends only what changed', async () => {
    const files = new FakeFiles();
    const first = await syncNote(files, encode('Milk'), 'small', 'watch-1', null, true);
    files.writes.length = 0;

    const edited = await syncNote(files, encode('Milk and eggs'), 'small', 'watch-1', first.onWatch, false);
    expect(files.writes).toEqual([NOTE_PATH]);

    files.writes.length = 0;
    await syncNote(files, encode('Milk and eggs'), 'medium', 'watch-1', edited.onWatch, false);
    expect(files.writes).toEqual([NOTE_FONT_PATH]);
  });

  it('sends both again to another watch, or when the watch lost them', async () => {
    const files = new FakeFiles();
    const first = await syncNote(files, encode('Milk'), 'small', 'watch-1', null, true);

    files.writes.length = 0;
    await syncNote(files, encode('Milk'), 'small', 'watch-2', first.onWatch, false);
    expect(files.writes).toEqual([NOTE_PATH, NOTE_FONT_PATH]);

    files.files.clear();
    files.writes.length = 0;
    await syncNote(files, encode('Milk'), 'small', 'watch-1', first.onWatch, true);
    expect(files.writes).toEqual([NOTE_PATH, NOTE_FONT_PATH]);
  });

  it('refuses a note too large for the watch', async () => {
    const files = new FakeFiles();

    await expect(
      syncNote(files, new Uint8Array(LARGEST_NOTE_BYTES + 1), 'normal', 'watch-1', null, true),
    ).rejects.toBeInstanceOf(NoteTooLarge);
    expect(files.writes).toEqual([]);
  });

  it('removes the note but leaves the font', async () => {
    const files = new FakeFiles();
    await syncNote(files, encode('Milk'), 'small', 'watch-1', null, true);

    await removeNote(files);

    expect(files.files.has(NOTE_PATH)).toBe(false);
    expect(files.files.has(NOTE_FONT_PATH)).toBe(true);
  });

  it('works over the file service, making the folder as it writes', async () => {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
    const service = await FileService.open(watch);
    const note = encode('A longer note.\n'.repeat(40));

    const first = await syncNote(service, note, 'normal', watch.deviceId, null, true);
    const again = await syncNote(service, note, 'normal', watch.deviceId, first.onWatch, true);

    expect(decode(await service.read(NOTE_PATH))).toBe(decode(note));
    expect(decode(await service.read(NOTE_FONT_PATH))).toBe('normal');
    expect([again.sentNote, again.sentFont]).toEqual([false, false]);
  });
});
