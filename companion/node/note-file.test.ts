import { afterEach, describe, expect, it } from '@jest/globals';
import { mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { NoteFileWatches, readNoteFile, watchNoteFile } = require('./note-file');

const SETTLE_MS = 50;

let folder = '';

function freshFolder(): string {
  folder = mkdtempSync(join(tmpdir(), 'neotime-note-'));
  return folder;
}

// Resolves once `calls` has been called, or with false after a while.
function calledWithin(milliseconds: number, calls: { count: number }): Promise<boolean> {
  return new Promise((resolve) => {
    const started = Date.now();
    const look = () => {
      if (calls.count > 0) {
        resolve(true);
      } else if (Date.now() - started > milliseconds) {
        resolve(false);
      } else {
        setTimeout(look, 20);
      }
    };
    look();
  });
}

afterEach(() => {
  rmSync(folder, { recursive: true, force: true });
});

describe('the note file on a computer', () => {
  it('reads the file as bytes', async () => {
    const path = join(freshFolder(), 'note.txt');
    writeFileSync(path, 'Milk\n');

    expect(new TextDecoder().decode(await readNoteFile(path))).toBe('Milk\n');
  });

  it('hears a save once, including one that renames a new file over the old', async () => {
    const path = join(freshFolder(), 'note.txt');
    writeFileSync(path, 'one');
    const calls = { count: 0 };
    const stop = watchNoteFile(path, () => calls.count++, SETTLE_MS);

    writeFileSync(join(folder, 'other.txt'), 'not the note');
    expect(await calledWithin(4 * SETTLE_MS, calls)).toBe(false);

    writeFileSync(join(folder, 'note.txt.tmp'), 'two');
    renameSync(join(folder, 'note.txt.tmp'), path);
    expect(await calledWithin(2000, calls)).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 4 * SETTLE_MS));
    expect(calls.count).toBe(1);
    stop();
  });

  it('numbers the watches and stops each one', async () => {
    const path = join(freshFolder(), 'note.txt');
    writeFileSync(path, 'one');
    const heard: number[] = [];
    const watches = new NoteFileWatches((id: number) => heard.push(id), SETTLE_MS);

    const first = watches.start(path);
    const second = watches.start(path);
    watches.stop(first);
    writeFileSync(path, 'two');
    await new Promise((resolve) => setTimeout(resolve, 1000));

    expect(heard).toEqual([second]);
    watches.stopAll();
    expect(watches.stops.size).toBe(0);
  });
});
