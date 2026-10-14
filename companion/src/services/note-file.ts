// The text file chosen to show on the watch: picking it, reading it again, and hearing when it changes.
//
// The app reaches the file through its bridge, so it reads the file's latest contents, and on the desktop hears
// each save. A plain browser keeps a link to the chosen file for as long as the page is open, which can only be
// read again, and is checked for changes from time to time instead.

import { chooseFiles } from './device-files';

// ============================================================================
// The chosen file
// ============================================================================

// Where the note comes from: a path on a computer, a content link on a phone, or a page's link in a browser.
export type NoteFileRef = { name: string; uri: string };

export type NoteFileAccess = {
  // Lets the user choose a file; none when they cancel.
  pick(): Promise<NoteFileRef | null>;
  read(file: NoteFileRef): Promise<Uint8Array>;
  // Calls `onChange` each time the file is saved, where the platform can tell; gives the call that stops it.
  watch(file: NoteFileRef, onChange: () => void): (() => void) | null;
};

// ============================================================================
// The app
// ============================================================================

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

// What window.neotime.notes (tauri/src/bridge.js) offers.
export type NoteBridge = {
  pick(): Promise<BridgeResult<NoteFileRef | null>>;
  read(path: string): Promise<BridgeResult<Uint8Array>>;
  watch(path: string): Promise<BridgeResult<number>>;
  unwatch(watchId: number): Promise<BridgeResult<void>>;
  onChange(listener: (watchId: number) => void): () => void;
};

export function noteBridge(): NoteBridge | null {
  return (globalThis as { neotime?: { notes?: NoteBridge } }).neotime?.notes ?? null;
}

function valueOf<Value>(result: BridgeResult<Value>): Value {
  if (!result.ok) {
    throw new Error(result.error ?? 'The app could not reach the file.');
  }
  return result.value as Value;
}

export function bridgeNoteAccess(bridge: NoteBridge): NoteFileAccess {
  return {
    async pick() {
      return valueOf(await bridge.pick());
    },
    async read(file) {
      return new Uint8Array(valueOf(await bridge.read(file.uri)));
    },
    watch(file, onChange) {
      let watchId: number | null = null;
      let stopped = false;
      const stopListening = bridge.onChange((changed) => {
        if (changed === watchId) {
          onChange();
        }
      });
      void bridge.watch(file.uri).then((result) => {
        watchId = result.ok ? (result.value ?? null) : null;
        if (stopped && watchId !== null) {
          void bridge.unwatch(watchId);
        }
      });
      return () => {
        stopped = true;
        stopListening();
        if (watchId !== null) {
          void bridge.unwatch(watchId);
        }
      };
    },
  };
}

// ============================================================================
// A plain browser
// ============================================================================

export const pickerNoteAccess: NoteFileAccess = {
  async pick() {
    const [file] = await chooseFiles({ accept: 'text/*' });
    return file ? { name: file.name, uri: URL.createObjectURL(file) } : null;
  },
  async read(file) {
    try {
      return new Uint8Array(await (await fetch(file.uri)).arrayBuffer());
    } catch {
      // A link from an earlier page load is gone.
      throw new Error(`${file.name} could not be read; choose it again.`);
    }
  },
  watch() {
    return null;
  },
};

export function noteFileAccess(): NoteFileAccess {
  const bridge = noteBridge();
  return bridge ? bridgeNoteAccess(bridge) : pickerNoteAccess;
}
