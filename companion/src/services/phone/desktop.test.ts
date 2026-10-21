import { afterEach, describe, expect, it, jest } from '@jest/globals';

import {
  choosePlayer,
  createDesktopPhoneBridge,
  DesktopEventTracker,
  desktopDeviceBridge,
  stripMarkup,
  type DesktopDeviceBridge,
  type RawDesktopEvent,
  type RawNotify,
  type RawPlayer,
} from './desktop';
import { NO_SOS_PERMISSIONS, type PhoneBridge, type PhoneEvent } from './types';
import { deviceWord } from './words';

// ============================================================================
// Helpers
// ============================================================================

function notify(fields: Partial<RawNotify> = {}): RawNotify {
  return {
    kind: 'notify',
    sender: ':1.40',
    serial: 7,
    appName: 'Thunderbird',
    replacesId: 0,
    summary: 'New mail',
    body: 'From <b>Ada</b> &amp; Grace',
    hints: {},
    ...fields,
  };
}

function player(fields: Partial<RawPlayer> = {}): RawPlayer {
  return {
    name: 'org.mpris.MediaPlayer2.firefox',
    status: 'Playing',
    metadata: {
      'xesam:artist': ['Nina Simone'],
      'xesam:album': 'Pastel Blues',
      'xesam:title': 'Sinnerman',
      'mpris:length': 622_000_000,
    },
    position: 61_400_000,
    volume: 0.5,
    ...fields,
  };
}

function track() {
  const events: PhoneEvent[] = [];
  const tracker = new DesktopEventTracker((event) => events.push(event));
  return { tracker, events };
}

// A bridge the test drives: it records commands and can send raw events.
function fakeDevice(answers: Partial<Record<'access' | 'quiet' | 'media' | 'locate', { ok: boolean; value?: unknown; error?: string }>> = {}) {
  let send: (event: RawDesktopEvent) => void = () => {};
  const device = {
    hasNotificationAccess: jest.fn(async () => answers.access ?? { ok: true, value: true }),
    isDoNotDisturb: jest.fn(async () => answers.quiet ?? { ok: true, value: false }),
    mediaCommand: jest.fn(async (_player: string, _command: string) => answers.media ?? { ok: true }),
    locate: jest.fn(async () => answers.locate ?? { ok: true, value: { latitude: 18, longitude: -76.8, description: '' } }),
    onEvent: jest.fn((listener: (event: RawDesktopEvent) => void) => {
      send = listener;
      return () => {};
    }),
  };
  return { device: device as unknown as DesktopDeviceBridge & typeof device, send: (event: RawDesktopEvent) => send(event) };
}

// ============================================================================
// Notifications
// ============================================================================

describe('stripMarkup', () => {
  it('drops tags and decodes the entities desktop notifications use', () => {
    expect(stripMarkup(' <i>a</i> &lt;b&gt; &quot;c&quot; &apos;d&apos; &amp; ')).toBe('a <b> "c" \'d\' &');
  });
});

describe('DesktopEventTracker notifications', () => {
  it('sends a notification once the server says which id it gave it', () => {
    const { tracker, events } = track();

    tracker.handle(notify());
    expect(events).toEqual([]);
    tracker.handle({ kind: 'notify-reply', destination: ':1.40', replySerial: 7, id: 12 });

    expect(events).toEqual([
      { type: 'notification', id: 12, app: 'Thunderbird', title: 'New mail', body: 'From Ada & Grace' },
    ]);
  });

  it('ignores answers to calls it never saw, and answers it already used', () => {
    const { tracker, events } = track();

    tracker.handle(notify());
    tracker.handle({ kind: 'notify-reply', destination: ':1.41', replySerial: 7, id: 12 });
    tracker.handle({ kind: 'notify-reply', destination: ':1.40', replySerial: 7, id: 12 });
    tracker.handle({ kind: 'notify-reply', destination: ':1.40', replySerial: 7, id: 12 });

    expect(events).toHaveLength(1);
  });

  it('sends a replacement at once under the id it replaces', () => {
    const { tracker, events } = track();

    tracker.handle(notify({ replacesId: 5, body: 'Two new messages' }));

    expect(events).toEqual([
      { type: 'notification', id: 5, app: 'Thunderbird', title: 'New mail', body: 'Two new messages' },
    ]);
  });

  it('names the app from its desktop entry when it gives no name', () => {
    const { tracker, events } = track();

    tracker.handle(notify({ appName: '', replacesId: 1, hints: { 'desktop-entry': 'org.gnome.Calendar' } }));

    expect(events[0]).toMatchObject({ app: 'org.gnome.Calendar' });
  });

  it('never sends the desktop app its own notifications', () => {
    const { tracker, events } = track();

    tracker.handle(notify({ appName: 'BattleTime Companion', replacesId: 3 }));

    expect(events).toEqual([]);
  });

  it('forgets the oldest notification still waiting when too many pile up', () => {
    const { tracker, events } = track();

    for (let serial = 0; serial <= 100; serial++) {
      tracker.handle(notify({ serial }));
    }
    tracker.handle({ kind: 'notify-reply', destination: ':1.40', replySerial: 0, id: 1 });
    tracker.handle({ kind: 'notify-reply', destination: ':1.40', replySerial: 100, id: 2 });

    expect(events.map((event) => (event as { id: number }).id)).toEqual([2]);
  });

  it('passes on a closed notification', () => {
    const { tracker, events } = track();

    tracker.handle({ kind: 'closed', id: 12 });

    expect(events).toEqual([{ type: 'notificationRemoved', id: 12 }]);
  });
});

// ============================================================================
// Media players
// ============================================================================

describe('choosePlayer', () => {
  const paused = player({ name: 'paused', status: 'Paused' });
  const playing = player({ name: 'playing' });
  const stopped = player({ name: 'stopped', status: 'Stopped' });

  it('prefers a player that is playing', () => {
    expect(choosePlayer([paused, playing], 'paused')).toBe(playing);
  });

  it('keeps following the current player while nothing plays', () => {
    const otherPaused = player({ name: 'other', status: 'Paused' });
    expect(choosePlayer([otherPaused, paused], 'paused')).toBe(paused);
  });

  it('falls back to a paused player, and to none when all are stopped', () => {
    expect(choosePlayer([stopped, paused], 'stopped')).toBe(paused);
    expect(choosePlayer([stopped], null)).toBeNull();
  });
});

describe('DesktopEventTracker media', () => {
  it('describes the chosen player in seconds', () => {
    const { tracker, events } = track();

    tracker.handle({ kind: 'players', players: [player()] });

    expect(events).toEqual([
      {
        type: 'media',
        playing: true,
        artist: 'Nina Simone',
        album: 'Pastel Blues',
        track: 'Sinnerman',
        durationSec: 622,
        positionSec: 61,
      },
    ]);
    expect(tracker.currentPlayer).toBe('org.mpris.MediaPlayer2.firefox');
    expect(tracker.media).toEqual(events[0]);
  });

  it('leaves out what a player does not report', () => {
    const { tracker, events } = track();

    tracker.handle({
      kind: 'players',
      players: [player({ metadata: { 'xesam:artist': [], 'xesam:title': '', 'mpris:length': -1 }, position: null })],
    });

    expect(events).toEqual([{ type: 'media', playing: true }]);
  });

  it('sends media only when it changes', () => {
    const { tracker, events } = track();

    tracker.handle({ kind: 'players', players: [player()] });
    tracker.handle({ kind: 'players', players: [player()] });
    tracker.handle({ kind: 'players', players: [player({ status: 'Paused' })] });

    expect(events.map((event) => (event as { playing: boolean }).playing)).toEqual([true, false]);
  });

  it('reports nothing playing once the last player goes', () => {
    const { tracker, events } = track();

    tracker.handle({ kind: 'players', players: [player()] });
    tracker.handle({ kind: 'players', players: [] });

    expect(events[1]).toEqual({ type: 'media', playing: false });
    expect(tracker.currentPlayer).toBeNull();
  });
});

// ============================================================================
// The bridge
// ============================================================================

describe('createDesktopPhoneBridge', () => {
  it('says it is a computer that reads notifications and media but has no calls', async () => {
    const { device } = fakeDevice();
    const bridge = createDesktopPhoneBridge(device);

    expect(bridge).toMatchObject({ available: true, kind: 'computer', supportsCalls: false, supportsLocation: true });
    await expect(bridge.hasNotificationAccess()).resolves.toBe(true);
    await expect(bridge.isDoNotDisturb()).resolves.toBe(false);
    await expect(bridge.requestCallPermissions()).resolves.toBe(false);
    await expect(bridge.answerCall()).rejects.toThrow('no calls');
    await expect(bridge.sosPermissions()).resolves.toEqual(NO_SOS_PERMISSIONS);
    await expect(bridge.openOverlaySettings()).rejects.toThrow('Android');
    await expect(bridge.rejectCall()).rejects.toThrow('no calls');
    await expect(bridge.silenceCall()).rejects.toThrow('no calls');
    await expect(bridge.openNotificationAccessSettings()).rejects.toThrow();
    await expect(bridge.associateWatch('AA')).resolves.toBe(false);
    await expect(bridge.isIgnoringBatteryOptimizations()).resolves.toBe(true);
    await expect(bridge.requestIgnoreBatteryOptimizations()).resolves.toBe(true);
  });

  it('passes on what the core reports, and stops when unsubscribed', () => {
    const { device, send } = fakeDevice();
    const bridge = createDesktopPhoneBridge(device);
    const listener = jest.fn();

    const unsubscribe = bridge.subscribe(listener);
    send({ kind: 'closed', id: 4 });
    unsubscribe();
    send({ kind: 'closed', id: 5 });

    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith({ type: 'notificationRemoved', id: 4 });
  });

  it('gives a late listener what is playing straight away', () => {
    const { device, send } = fakeDevice();
    const bridge = createDesktopPhoneBridge(device);
    send({ kind: 'players', players: [player()] });
    const listener = jest.fn();

    bridge.subscribe(listener);

    expect(listener).toHaveBeenCalledWith(expect.objectContaining({ type: 'media', track: 'Sinnerman' }));
  });

  it('drives the player it follows, and refuses when there is none', async () => {
    const { device, send } = fakeDevice();
    const bridge = createDesktopPhoneBridge(device);

    await expect(bridge.mediaCommand('play')).rejects.toThrow('No media player');
    send({ kind: 'players', players: [player()] });
    await bridge.mediaCommand('next');

    expect(device.mediaCommand).toHaveBeenCalledWith('org.mpris.MediaPlayer2.firefox', 'next');
  });

  it('turns a failed request into an error', async () => {
    const { device } = fakeDevice({ quiet: { ok: false, error: 'no bus' }, access: { ok: false } });
    const bridge = createDesktopPhoneBridge(device);

    await expect(bridge.isDoNotDisturb()).rejects.toThrow('no bus');
    await expect(bridge.hasNotificationAccess()).rejects.toThrow('desktop bridge failed');
  });

  it('finds where the computer is, naming it Here when the service gives no name', async () => {
    const unnamed = createDesktopPhoneBridge(fakeDevice().device);
    await expect(unnamed.locate()).resolves.toEqual({ latitude: 18, longitude: -76.8, name: 'Here' });

    const named = createDesktopPhoneBridge(
      fakeDevice({ locate: { ok: true, value: { latitude: 1, longitude: 2, description: 'Kingston' } } }).device,
    );
    await expect(named.locate()).resolves.toMatchObject({ name: 'Kingston' });
  });
});

describe('choosing the bridge', () => {
  afterEach(() => {
    delete (globalThis as { battletime?: unknown }).battletime;
  });

  it('finds the bridge only in the desktop app', () => {
    expect(desktopDeviceBridge()).toBeNull();
    const { device } = fakeDevice();
    (globalThis as { battletime?: unknown }).battletime = { desktop: device };
    expect(desktopDeviceBridge()).toBe(device);
  });

  it('gives the desktop app the computer bridge', () => {
    (globalThis as { battletime?: unknown }).battletime = { desktop: fakeDevice().device };
    let bridge!: PhoneBridge;
    jest.isolateModules(() => {
      bridge = jest.requireActual<typeof import('./index')>('./index').phoneBridge;
    });
    expect(bridge.kind).toBe('computer');
  });

  it('calls the device a computer or a phone', () => {
    expect(deviceWord('computer')).toBe('computer');
    expect(deviceWord('phone')).toBe('phone');
    expect(deviceWord('none')).toBe('phone');
  });
});
