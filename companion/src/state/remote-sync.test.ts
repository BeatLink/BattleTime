import { describe, expect, it } from '@jest/globals';

import type { WatchFile } from '@/protocol/file-service';
import { REMOTE_BUTTONS_PATH, type RemoteButton, type RemoteFiles, type RemoteOnWatch } from '@/protocol/remote';

import { RemoteSync, type RemoteStatus } from './remote-sync';

class FakeWatchFiles implements RemoteFiles {
  readonly files = new Map<string, Uint8Array>();
  writes = 0;
  lists = 0;
  gate: Promise<void> | null = null;
  fail: Error | null = null;

  async write(path: string, contents: Uint8Array) {
    if (this.fail) throw this.fail;
    this.writes++;
    await this.gate;
    this.files.set(path, contents);
  }

  async list(dir: string): Promise<WatchFile[]> {
    this.lists++;
    return [...this.files]
      .filter(([path]) => path.startsWith(`${dir}/`))
      .map(([path, data]) => ({ name: path, path, size: data.length, directory: false }));
  }
}

const PLAY: RemoteButton = { icon: 'play', label: '', action: { kind: 'builtin', name: 'playpause' } };
const NEXT: RemoteButton = { icon: 'next', label: '', action: { kind: 'builtin', name: 'next' } };

function setUp(buttons: RemoteButton[]) {
  const watch = new FakeWatchFiles();
  const state = { buttons, onWatch: null as RemoteOnWatch | null };
  const statuses: RemoteStatus[] = [];
  const sync = new RemoteSync({
    files: async () => watch,
    watchId: 'watch',
    current: () => state,
    remember: (onWatch) => (state.onWatch = onWatch),
    onStatus: (status) => statuses.push(status),
    now: () => 5,
  });
  return { watch, state, statuses, sync };
}

describe('keeping the remote layout on the watch', () => {
  it('sends the layout once, then only when the buttons change', async () => {
    const { watch, state, statuses, sync } = setUp([PLAY]);
    await sync.sync();
    expect(watch.writes).toBe(1);
    expect(watch.lists).toBe(0);
    expect(state.onWatch?.watchId).toBe('watch');
    expect(statuses.at(-1)).toEqual({ kind: 'synced', at: 5, buttons: 1 });

    await sync.sync();
    expect(watch.writes).toBe(1);
    state.buttons = [PLAY, NEXT];
    await sync.sync();
    expect(watch.writes).toBe(2);
    expect(Array.from(watch.files.get(REMOTE_BUTTONS_PATH)!)).toHaveLength(4);
  });

  it('checks the watch first on a new connection and resends a layout that is gone', async () => {
    const first = setUp([PLAY]);
    await first.sync.sync();
    const again = setUp([PLAY]);
    again.state.onWatch = first.state.onWatch;
    await again.sync.sync();
    expect(again.watch.lists).toBe(1);
    expect(again.watch.writes).toBe(1);
  });

  it('writes on demand even when nothing changed', async () => {
    const { watch, sync } = setUp([PLAY]);
    await sync.sync();
    await sync.sync(true);
    expect(watch.writes).toBe(2);
  });

  it('never overlaps sends, and makes one more for a change asked for during one', async () => {
    const { watch, state, sync } = setUp([PLAY]);
    let open!: () => void;
    watch.gate = new Promise((resolve) => (open = resolve));
    const running = sync.sync();
    state.buttons = [NEXT];
    const queued = sync.sync();
    expect(queued).toBe(running);
    open();
    await running;
    expect(watch.writes).toBe(2);
  });

  it('reports a failure, such as more buttons than the watch shows', async () => {
    const { statuses, sync } = setUp(Array.from({ length: 10 }, () => PLAY));
    await sync.sync();
    expect(statuses.at(-1)).toMatchObject({ kind: 'failed', message: expect.stringContaining('up to 9') });
  });

  it('reports a write the watch refuses', async () => {
    const { watch, statuses, sync } = setUp([PLAY]);
    watch.fail = new Error('link lost');
    await sync.sync();
    expect(statuses.at(-1)).toEqual({ kind: 'failed', message: 'link lost', at: 5 });
    watch.fail = null;
    await sync.sync();
    expect(statuses.at(-1)).toMatchObject({ kind: 'synced' });
  });
});
