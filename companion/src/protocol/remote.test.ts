import { describe, expect, it } from '@jest/globals';

import { LIBRARY_ICON_NAMES } from '@/bundles/icon-names';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { FileService, type WatchFile } from './file-service';
import { decodeWatchLine, type WatchToPhoneMessage } from './gadgetbridge';
import {
  decodeRemoteLayout,
  encodeRemoteLayout,
  readRemoteAction,
  readRemoteButtons,
  remoteTapIndex,
  REMOTE_BUTTONS_PATH,
  RemoteLayoutError,
  syncRemoteLayout,
  type RemoteButton,
  type RemoteFiles,
} from './remote';

const icon = (name: string) => LIBRARY_ICON_NAMES.indexOf(name as (typeof LIBRARY_ICON_NAMES)[number]);

const PLAY: RemoteButton = { icon: 'play', label: 'Play', action: { kind: 'builtin', name: 'playpause' } };
const LIGHTS: RemoteButton = {
  icon: 'flashlight',
  label: 'Lights',
  action: { kind: 'http', method: 'POST', url: 'https://example.com/lights', body: 'on' },
};

describe('the remote layout file', () => {
  it('holds the version, the count and each icon as its place in the library', () => {
    expect(Array.from(encodeRemoteLayout(['play', 'next', 'alarm']))).toEqual([1, 3, icon('play'), icon('next'), 0]);
    expect(Array.from(encodeRemoteLayout([]))).toEqual([1, 0]);
  });

  it('numbers the last icon in the library by its place too', () => {
    const last = LIBRARY_ICON_NAMES[LIBRARY_ICON_NAMES.length - 1];
    expect(Array.from(encodeRemoteLayout([last]))).toEqual([1, 1, LIBRARY_ICON_NAMES.length - 1]);
  });

  it('takes nine buttons but refuses a tenth', () => {
    const nine = Array.from({ length: 9 }, () => 'play');
    expect(encodeRemoteLayout(nine)).toHaveLength(11);
    expect(() => encodeRemoteLayout([...nine, 'play'])).toThrow(RemoteLayoutError);
    expect(() => encodeRemoteLayout([...nine, 'play'])).toThrow('up to 9 buttons, not 10');
  });

  it('refuses an icon the library does not have', () => {
    expect(() => encodeRemoteLayout(['play', 'teapot'])).toThrow('no icon called teapot');
  });

  it('reads a layout back into names', () => {
    expect(decodeRemoteLayout(encodeRemoteLayout(['bell', 'music']))).toEqual(['bell', 'music']);
  });

  it('refuses a layout of another version, the wrong length or an unknown icon', () => {
    expect(() => decodeRemoteLayout(Uint8Array.from([2, 0]))).toThrow('version 1');
    expect(() => decodeRemoteLayout(Uint8Array.from([1]))).toThrow('version 1');
    expect(() => decodeRemoteLayout(Uint8Array.from([1, 2, 0]))).toThrow('wrong length');
    expect(() => decodeRemoteLayout(Uint8Array.from([1, 10, ...new Array(10).fill(0)]))).toThrow('wrong length');
    expect(() => decodeRemoteLayout(Uint8Array.from([1, 1, 250]))).toThrow('icon 250');
  });
});

describe('reading stored buttons', () => {
  it('keeps well-formed buttons of every kind', () => {
    const buttons: RemoteButton[] = [
      PLAY,
      LIGHTS,
      { icon: 'settings', label: '', action: { kind: 'command', command: 'notify-send hi' } },
      { icon: 'navigation', label: 'Map', action: { kind: 'android', intent: { type: 'deeplink', uri: 'geo:0,0' } } },
      {
        icon: 'music',
        label: 'Player',
        action: {
          kind: 'android',
          intent: { type: 'activity', packageName: 'org.example', className: 'org.example.Main', action: 'GO', extras: { a: '1' } },
        },
      },
      {
        icon: 'bell',
        label: 'Ping',
        action: { kind: 'android', intent: { type: 'broadcast', action: 'org.example.PING', packageName: 'org.example', extras: { b: '2' } } },
      },
    ];
    expect(readRemoteButtons(JSON.parse(JSON.stringify(buttons)))).toEqual(buttons);
  });

  it('drops malformed buttons and anything past the ninth', () => {
    const stored = [
      PLAY,
      null,
      'play',
      { icon: 'teapot', label: 'x', action: PLAY.action },
      { icon: 'play', action: { kind: 'shell', command: 'rm -rf /' } },
      { icon: 'play', label: 5, action: { kind: 'builtin', name: 'next' } },
      ...Array.from({ length: 10 }, () => PLAY),
    ];
    const read = readRemoteButtons(stored);
    expect(read).toHaveLength(9);
    expect(read[1]).toEqual({ icon: 'play', label: '', action: { kind: 'builtin', name: 'next' } });
    expect(readRemoteButtons('nope')).toEqual([]);
  });

  it('checks each action field and drops empty optional ones', () => {
    expect(readRemoteAction(null)).toBeNull();
    expect(readRemoteAction({ kind: 'http', method: 'PATCH', url: 'https://x' })).toBeNull();
    expect(readRemoteAction({ kind: 'http', method: 'GET', url: 'https://x', headers: { A: 'b', C: 3 }, body: '' })).toEqual({
      kind: 'http',
      method: 'GET',
      url: 'https://x',
      headers: { A: 'b' },
    });
    expect(readRemoteAction({ kind: 'http', method: 'GET', url: 'https://x', headers: [] })).toEqual({
      kind: 'http',
      method: 'GET',
      url: 'https://x',
    });
    expect(readRemoteAction({ kind: 'command', command: 4 })).toBeNull();
    expect(readRemoteAction({ kind: 'builtin', name: 'explode' })).toBeNull();
    expect(readRemoteAction({ kind: 'android' })).toBeNull();
    expect(readRemoteAction({ kind: 'android', intent: { type: 'deeplink' } })).toBeNull();
    expect(readRemoteAction({ kind: 'android', intent: { type: 'activity', packageName: 'a' } })).toBeNull();
    expect(readRemoteAction({ kind: 'android', intent: { type: 'broadcast' } })).toBeNull();
    expect(readRemoteAction({ kind: 'android', intent: { type: 'service' } })).toBeNull();
    expect(readRemoteAction({ kind: 'android', intent: { type: 'broadcast', action: 'A', packageName: '' } })).toEqual({
      kind: 'android',
      intent: { type: 'broadcast', action: 'A' },
    });
  });
});

describe('sending the layout', () => {
  class FakeFiles implements RemoteFiles {
    readonly files = new Map<string, Uint8Array>();
    writes = 0;
    failList = false;

    async write(path: string, contents: Uint8Array) {
      this.writes++;
      this.files.set(path, contents);
    }

    async list(dir: string): Promise<WatchFile[]> {
      if (this.failList) throw new Error('no such folder');
      return [...this.files]
        .filter(([path]) => path.startsWith(`${dir}/`))
        .map(([path, data]) => ({ name: path, path, size: data.length, directory: false }));
    }
  }

  it('writes the layout once and not again while it is unchanged', async () => {
    const files = new FakeFiles();
    const first = await syncRemoteLayout(files, [PLAY, LIGHTS], 'watch', null, true);
    expect(first.sent).toBe(true);
    expect(Array.from(files.files.get(REMOTE_BUTTONS_PATH)!)).toEqual([1, 2, icon('play'), icon('flashlight')]);
    const again = await syncRemoteLayout(files, [PLAY, LIGHTS], 'watch', first.onWatch, true);
    expect(again.sent).toBe(false);
    expect(files.writes).toBe(1);
  });

  it('writes again for a changed layout, another watch, or a layout gone from the watch', async () => {
    const files = new FakeFiles();
    const { onWatch } = await syncRemoteLayout(files, [PLAY], 'watch', null, false);
    expect((await syncRemoteLayout(files, [LIGHTS], 'watch', onWatch, false)).sent).toBe(true);
    expect((await syncRemoteLayout(files, [PLAY], 'other', onWatch, false)).sent).toBe(true);
    files.files.clear();
    expect((await syncRemoteLayout(files, [PLAY], 'watch', onWatch, true)).sent).toBe(true);
    files.failList = true;
    expect((await syncRemoteLayout(files, [PLAY], 'watch', onWatch, true)).sent).toBe(true);
  });

  it('reaches the mock watch through the file service', async () => {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = await FileService.open(watch);
    await syncRemoteLayout(service, [PLAY], watch.deviceId, null, true);
    expect(decodeRemoteLayout(await service.read(REMOTE_BUTTONS_PATH))).toEqual(['play']);
  });
});

describe('taps from the watch', () => {
  const tap = (line: string) => remoteTapIndex(decodeWatchLine(line) as WatchToPhoneMessage);

  it('reads the button index from a remote tap line', () => {
    expect(tap('{"t":"app","ch":"remote","d":{"button":0}}')).toBe(0);
    expect(tap('{"t":"app","ch":"remote","d":{"button":8}}\r')).toBe(8);
  });

  it('ignores other channels, other messages and malformed taps', () => {
    expect(tap('{"t":"app","ch":"timer","d":{"button":1}}')).toBeNull();
    expect(tap('{"t":"music","n":"play"}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote"}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote","d":3}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote","d":{"button":"1"}}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote","d":{"button":-1}}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote","d":{"button":1.5}}')).toBeNull();
    expect(tap('{"t":"app","ch":"remote","d":{"button":1,"url":"https://evil"}}')).toBe(1);
  });
});
