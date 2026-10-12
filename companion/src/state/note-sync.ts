// Keeps the watch's note matching the chosen file while a watch is connected.
//
// Each check reads the file again and sends it, or its font, only when it changed. The first check
// on a connection also lists the watch's files, so a note removed or replaced there is sent again.
// Checks never overlap: one asked for while another runs is made once that one ends.

import { fingerprint, syncNote, type NoteFiles, type NoteFont, type NoteOnWatch } from '@/protocol/notes';
import type { NoteFileAccess, NoteFileRef } from '@/services/note-file';

// ============================================================================
// What the screen shows
// ============================================================================

export type NoteStatus =
  | { kind: 'idle' }
  | { kind: 'sending'; bytes: number }
  | { kind: 'synced'; checkedAt: number; bytes: number }
  | { kind: 'failed'; message: string; at: number };

// ============================================================================
// Checking and sending
// ============================================================================

export type NoteSyncParts = {
  access: NoteFileAccess;
  files: () => Promise<NoteFiles>;
  watchId: string;
  // The chosen file, its font and what the watch was last sent, as the settings hold them now.
  current: () => { file: NoteFileRef | null; font: NoteFont; onWatch: NoteOnWatch | null };
  remember: (onWatch: NoteOnWatch) => void;
  onStatus: (status: NoteStatus) => void;
  now?: () => number;
};

export class NoteSync {
  private running: Promise<void> | null = null;
  private again = false;
  private listedWatch = false;
  // What this connection last sent, which the settings only hold once the screen has drawn again.
  private sent: NoteOnWatch | null = null;

  constructor(private readonly parts: NoteSyncParts) {}

  // Checks the file and sends what changed; a call made while a check runs is answered by one more check after it.
  sync(): Promise<void> {
    if (this.running) {
      this.again = true;
      return this.running;
    }
    this.running = (async () => {
      do {
        this.again = false;
        await this.checkOnce();
      } while (this.again);
    })().finally(() => {
      this.running = null;
    });
    return this.running;
  }

  private async checkOnce(): Promise<void> {
    const { access, files, watchId, current, remember, onStatus } = this.parts;
    const now = this.parts.now ?? Date.now;
    const { file, font, onWatch: remembered } = current();
    const onWatch = this.sent ?? remembered;
    if (!file) {
      onStatus({ kind: 'idle' });
      return;
    }
    try {
      const note = await access.read(file);
      const changed =
        onWatch === null ||
        onWatch.watchId !== watchId ||
        onWatch.fingerprint !== fingerprint(note) ||
        onWatch.font !== font;
      if (changed) {
        onStatus({ kind: 'sending', bytes: note.length });
      }
      const result = await syncNote(await files(), note, font, watchId, onWatch, !this.listedWatch);
      this.listedWatch = true;
      if (result.sentNote || result.sentFont) {
        this.sent = result.onWatch;
        remember(result.onWatch);
      }
      onStatus({ kind: 'synced', checkedAt: now(), bytes: note.length });
    } catch (error) {
      onStatus({ kind: 'failed', message: error instanceof Error ? error.message : String(error), at: now() });
    }
  }
}
