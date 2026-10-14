// Keeps the watch's remote layout matching the buttons set up here while a watch is connected.
//
// The first send on a connection lists the watch's /remote folder, so a layout removed there is sent again.
// Sends never overlap: one asked for while another runs is made once that one ends.

import { syncRemoteLayout, type RemoteButton, type RemoteFiles, type RemoteOnWatch } from '@/protocol/remote';

// ============================================================================
// What the screen shows
// ============================================================================

export type RemoteStatus =
  | { kind: 'idle' }
  | { kind: 'sending'; buttons: number }
  | { kind: 'synced'; at: number; buttons: number }
  | { kind: 'failed'; message: string; at: number };

// ============================================================================
// Sending
// ============================================================================

export type RemoteSyncParts = {
  files: () => Promise<RemoteFiles>;
  watchId: string;
  // The buttons and what the watch was last sent, as the settings hold them now.
  current: () => { buttons: RemoteButton[]; onWatch: RemoteOnWatch | null };
  remember: (onWatch: RemoteOnWatch) => void;
  onStatus: (status: RemoteStatus) => void;
  now?: () => number;
};

export class RemoteSync {
  private running: Promise<void> | null = null;
  private again = false;
  private listedWatch = false;
  // What this connection last sent, which the settings only hold once the screen has drawn again.
  private sent: RemoteOnWatch | null = null;
  // A send asked for by hand writes the file even when it looks unchanged.
  private force = false;

  constructor(private readonly parts: RemoteSyncParts) {}

  // Sends the layout if it changed; with `force` it is written whatever the watch was last sent.
  sync(force = false): Promise<void> {
    this.force = this.force || force;
    if (this.running) {
      this.again = true;
      return this.running;
    }
    this.running = (async () => {
      do {
        this.again = false;
        await this.sendOnce();
      } while (this.again);
    })().finally(() => {
      this.running = null;
    });
    return this.running;
  }

  private async sendOnce(): Promise<void> {
    const { files, watchId, current, remember, onStatus } = this.parts;
    const now = this.parts.now ?? Date.now;
    const { buttons, onWatch: remembered } = current();
    const last = this.force ? null : (this.sent ?? remembered);
    this.force = false;
    try {
      onStatus({ kind: 'sending', buttons: buttons.length });
      const result = await syncRemoteLayout(await files(), buttons, watchId, last, !this.listedWatch);
      this.listedWatch = true;
      if (result.sent) {
        this.sent = result.onWatch;
        remember(result.onWatch);
      }
      onStatus({ kind: 'synced', at: now(), buttons: buttons.length });
    } catch (error) {
      onStatus({ kind: 'failed', message: error instanceof Error ? error.message : String(error), at: now() });
    }
  }
}
