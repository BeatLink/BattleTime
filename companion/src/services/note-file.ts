// The text file chosen to show on the watch: picking it, reading it again, and hearing when it changes.
//
// The desktop app reaches the real file on disk through the Electron bridge, so it reads the file's
// latest contents and hears each save. A phone or a browser keeps the document picker's link to it,
// which can only be read again, and is checked for changes from time to time instead.

import * as DocumentPicker from 'expo-document-picker';

// ============================================================================
// The chosen file
// ============================================================================

// Where the note comes from: a path on a computer, or the document picker's link on a phone.
export type NoteFileRef = { name: string; uri: string };

export type NoteFileAccess = {
  // Lets the user choose a file; none when they cancel.
  pick(): Promise<NoteFileRef | null>;
  read(file: NoteFileRef): Promise<Uint8Array>;
  // Calls `onChange` each time the file is saved, where the platform can tell; gives the call that stops it.
  watch(file: NoteFileRef, onChange: () => void): (() => void) | null;
};

// ============================================================================
// The desktop app
// ============================================================================

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

// What electron/preload.js offers for notes.
export type DesktopNoteBridge = {
  pick(): Promise<BridgeResult<NoteFileRef | null>>;
  read(path: string): Promise<BridgeResult<Uint8Array>>;
  watch(path: string): Promise<BridgeResult<number>>;
  unwatch(watchId: number): Promise<BridgeResult<void>>;
  onChange(listener: (watchId: number) => void): () => void;
};

export function desktopNoteBridge(): DesktopNoteBridge | null {
  return (globalThis as { neotime?: { notes?: DesktopNoteBridge } }).neotime?.notes ?? null;
}

function valueOf<Value>(result: BridgeResult<Value>): Value {
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop app could not reach the file.');
  }
  return result.value as Value;
}

export function desktopNoteAccess(bridge: DesktopNoteBridge): NoteFileAccess {
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
// A phone or a browser
// ============================================================================

export const pickerNoteAccess: NoteFileAccess = {
  async pick() {
    // The original is kept rather than a copy, so a later read sees the file's edits where the phone allows it.
    const picked = await DocumentPicker.getDocumentAsync({ type: 'text/*', copyToCacheDirectory: false });
    const asset = picked.canceled ? null : picked.assets?.[0];
    return asset ? { name: asset.name, uri: asset.uri } : null;
  },
  async read(file) {
    try {
      return new Uint8Array(await (await fetch(file.uri)).arrayBuffer());
    } catch {
      // The phone may have withdrawn the app's access to the file since it was chosen.
      throw new Error(`${file.name} could not be read; choose it again.`);
    }
  },
  watch() {
    return null;
  },
};

export function noteFileAccess(): NoteFileAccess {
  const bridge = desktopNoteBridge();
  return bridge ? desktopNoteAccess(bridge) : pickerNoteAccess;
}
