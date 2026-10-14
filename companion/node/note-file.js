// The text file the desktop app shows on the watch: read from disk, and watched for saves.
//
// Runs in the Electron main process. Editors often save by writing a new file and renaming it over
// the old one, which ends a watch on the file itself, so the folder is watched and its events are
// filtered by name.

const { watch } = require('node:fs');
const { readFile } = require('node:fs/promises');
const { basename, dirname } = require('node:path');

// Saves arrive as several events; they are answered once things are quiet for this long.
const SETTLE_MS = 300;

async function readNoteFile(path) {
  return new Uint8Array(await readFile(path));
}

// Calls `onChange` once after each save of the file at `path`; gives the call that stops watching.
function watchNoteFile(path, onChange, settleMs = SETTLE_MS) {
  const name = basename(path);
  let timer = null;
  const watcher = watch(dirname(path), (_event, changed) => {
    if (changed !== null && changed.toString() !== name) {
      return;
    }
    clearTimeout(timer);
    timer = setTimeout(onChange, settleMs);
  });
  watcher.on('error', () => undefined);
  return () => {
    clearTimeout(timer);
    watcher.close();
  };
}

// Keeps the window's watches by number, so the window can stop one it no longer needs.
class NoteFileWatches {
  constructor(emit, settleMs = SETTLE_MS) {
    this.emit = emit;
    this.settleMs = settleMs;
    this.next = 1;
    this.stops = new Map();
  }

  start(path) {
    const id = this.next++;
    this.stops.set(id, watchNoteFile(path, () => this.emit(id), this.settleMs));
    return id;
  }

  stop(id) {
    this.stops.get(id)?.();
    this.stops.delete(id);
  }

  stopAll() {
    for (const id of [...this.stops.keys()]) {
      this.stop(id);
    }
  }
}

module.exports = { readNoteFile, watchNoteFile, NoteFileWatches };
