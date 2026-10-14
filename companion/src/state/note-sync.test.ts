import { describe, expect, it } from '@jest/globals';

import type { WatchFile } from '@/protocol/file-service';
import { NOTE_FONT_PATH, NOTE_PATH, type NoteFiles, type NoteFont, type NoteOnWatch } from '@/protocol/notes';
import type { NoteReader } from '@/services/note-source';

import { NoteSync, type NoteStatus } from './note-sync';

// ============================================================================
// A watch, a file and settings, all in memory
// ============================================================================

class FakeWatchFiles implements NoteFiles {
  readonly files = new Map<string, Uint8Array>();
  readonly writes: string[] = [];
  lists = 0;
  // Holds each write until the test lets it finish, when set.
  gate: Promise<void> | null = null;

  async write(path: string, contents: Uint8Array): Promise<void> {
    this.writes.push(path);
    await this.gate;
    this.files.set(path, contents);
  }

  async list(dir: string): Promise<WatchFile[]> {
    this.lists++;
    return [...this.files]
      .filter(([path]) => path.startsWith(`${dir}/`))
      .map(([path, data]) => ({ name: path, path, size: data.length, directory: false }));
  }

  async remove(path: string): Promise<void> {
    this.files.delete(path);
  }
}

function setUp(text: string, hasSource = true) {
  const watchFiles = new FakeWatchFiles();
  const disk = { text, reads: 0, fails: false };
  const read: NoteReader = async () => {
    disk.reads++;
    if (disk.fails) {
      throw new Error('note.txt could not be read; choose it again.');
    }
    return new TextEncoder().encode(disk.text);
  };
  const settings: { read: NoteReader | null; font: NoteFont; onWatch: NoteOnWatch | null } = {
    read: hasSource ? read : null,
    font: 'normal',
    onWatch: null,
  };
  const statuses: NoteStatus[] = [];
  const sync = new NoteSync({
    files: async () => watchFiles,
    watchId: 'watch-1',
    current: () => settings,
    remember: (onWatch) => {
      settings.onWatch = onWatch;
    },
    onStatus: (status) => statuses.push(status),
    now: () => 1000,
  });
  return { sync, watchFiles, disk, settings, statuses };
}

const text = (bytes: Uint8Array | undefined) => new TextDecoder().decode(bytes);

// ============================================================================
// Checks
// ============================================================================

describe('NoteSync', () => {
  it('sends the chosen note and its font, and remembers what it sent', async () => {
    const { sync, watchFiles, settings, statuses } = setUp('Milk');

    await sync.sync();

    expect(text(watchFiles.files.get(NOTE_PATH))).toBe('Milk');
    expect(text(watchFiles.files.get(NOTE_FONT_PATH))).toBe('normal');
    expect(settings.onWatch?.watchId).toBe('watch-1');
    expect(statuses).toEqual([
      { kind: 'sending', bytes: 4 },
      { kind: 'synced', checkedAt: 1000, bytes: 4 },
    ]);
  });

  it('lists the watch only on the first check, and sends nothing while nothing changes', async () => {
    const { sync, watchFiles, statuses } = setUp('Milk');
    await sync.sync();
    watchFiles.writes.length = 0;
    statuses.length = 0;

    await sync.sync();
    await sync.sync();

    expect(watchFiles.lists).toBe(1);
    expect(watchFiles.writes).toEqual([]);
    expect(statuses.every((status) => status.kind === 'synced')).toBe(true);
  });

  it('sends an edited note, and a new font on its own', async () => {
    const { sync, watchFiles, disk, settings } = setUp('Milk');
    await sync.sync();
    watchFiles.writes.length = 0;

    disk.text = 'Milk and eggs';
    await sync.sync();
    settings.font = 'large';
    await sync.sync();

    expect(watchFiles.writes).toEqual([NOTE_PATH, NOTE_FONT_PATH]);
    expect(text(watchFiles.files.get(NOTE_PATH))).toBe('Milk and eggs');
  });

  it('runs a check asked for during another once that one ends, and never two at once', async () => {
    const { sync, watchFiles, disk } = setUp('Milk');
    let open: () => void = () => undefined;
    watchFiles.gate = new Promise((resolve) => {
      open = resolve;
    });

    const first = sync.sync();
    disk.text = 'Milk and eggs';
    const second = sync.sync();
    const third = sync.sync();
    await Promise.resolve();
    open();
    await Promise.all([first, second, third]);

    expect(disk.reads).toBe(2);
    expect(text(watchFiles.files.get(NOTE_PATH))).toBe('Milk and eggs');
  });

  it('reports a source it cannot read, and does nothing without one', async () => {
    const failing = setUp('Milk');
    failing.disk.fails = true;
    await failing.sync.sync();
    expect(failing.statuses).toEqual([
      { kind: 'failed', message: 'note.txt could not be read; choose it again.', at: 1000 },
    ]);

    const none = setUp('Milk', false);
    await none.sync.sync();
    expect(none.statuses).toEqual([{ kind: 'idle' }]);
    expect(none.watchFiles.writes).toEqual([]);
  });
});
