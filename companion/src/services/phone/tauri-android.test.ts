import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { createTauriPhoneBridge, tauriIntents, tauriPhoneDevice, type TauriPhoneDevice } from './tauri-android';
import type { PhoneBridge, PhoneEvent } from './types';

// ============================================================================
// Helpers
// ============================================================================

type Answer = { ok: boolean; value?: unknown; error?: string };

// A bridge the test drives: every command answers `answer`, and `send` delivers a raw event.
function fakeDevice(answers: Partial<Record<keyof TauriPhoneDevice, Answer>> = {}) {
  let send: (raw: unknown) => void = () => {};
  const stop = jest.fn();
  const answer = (name: keyof TauriPhoneDevice) =>
    jest.fn(async (..._args: unknown[]) => answers[name] ?? { ok: true });
  const device = {
    hasNotificationAccess: answer('hasNotificationAccess'),
    openNotificationAccessSettings: answer('openNotificationAccessSettings'),
    requestCallPermissions: answer('requestCallPermissions'),
    isDoNotDisturb: answer('isDoNotDisturb'),
    answerCall: answer('answerCall'),
    rejectCall: answer('rejectCall'),
    silenceCall: answer('silenceCall'),
    mediaCommand: answer('mediaCommand'),
    startKeepAlive: answer('startKeepAlive'),
    stopKeepAlive: answer('stopKeepAlive'),
    locate: answer('locate'),
    openUri: answer('openUri'),
    startActivity: answer('startActivity'),
    sendBroadcast: answer('sendBroadcast'),
    onEvent: jest.fn((listener: (raw: unknown) => void) => {
      send = listener;
      return stop;
    }),
  };
  return { device: device as unknown as TauriPhoneDevice, calls: device, send: (raw: unknown) => send(raw), stop };
}

afterEach(() => {
  delete (globalThis as { battletime?: unknown }).battletime;
});

// ============================================================================
// Finding the bridge
// ============================================================================

describe('tauriPhoneDevice', () => {
  it('is there only when the Tauri app on Android gives the window a phone', () => {
    expect(tauriPhoneDevice()).toBeNull();
    (globalThis as { battletime?: unknown }).battletime = { desktop: {} };
    expect(tauriPhoneDevice()).toBeNull();
    const { device } = fakeDevice();
    (globalThis as { battletime?: unknown }).battletime = { phone: device };
    expect(tauriPhoneDevice()).toBe(device);
  });

  it('is what index.ts chooses ahead of the desktop and the stub', () => {
    const { device } = fakeDevice();
    (globalThis as { battletime?: unknown }).battletime = { phone: device };
    let bridge!: PhoneBridge;
    jest.isolateModules(() => {
      bridge = jest.requireActual<typeof import('./index')>('./index').phoneBridge;
    });
    expect(bridge.kind).toBe('phone');
    expect(bridge.available).toBe(true);
  });
});

// ============================================================================
// The bridge
// ============================================================================

describe('createTauriPhoneBridge', () => {
  it('is a phone that follows calls and can find itself', () => {
    const bridge = createTauriPhoneBridge(fakeDevice().device);
    expect(bridge.kind).toBe('phone');
    expect(bridge.supportsCalls).toBe(true);
    expect(bridge.supportsLocation).toBe(true);
  });

  it('answers queries with the plugin values', async () => {
    const { device } = fakeDevice({
      hasNotificationAccess: { ok: true, value: true },
      requestCallPermissions: { ok: true, value: false },
      isDoNotDisturb: { ok: true, value: true },
    });
    const bridge = createTauriPhoneBridge(device);
    await expect(bridge.hasNotificationAccess()).resolves.toBe(true);
    await expect(bridge.requestCallPermissions()).resolves.toBe(false);
    await expect(bridge.isDoNotDisturb()).resolves.toBe(true);
  });

  it('passes actions on and turns a refusal into an error', async () => {
    const { device, calls } = fakeDevice({ answerCall: { ok: false, error: 'Allow the app to answer calls first' } });
    const bridge = createTauriPhoneBridge(device);
    await bridge.mediaCommand('next');
    await bridge.startKeepAlive('PineTime');
    await bridge.stopKeepAlive();
    await bridge.rejectCall();
    await bridge.silenceCall();
    await bridge.openNotificationAccessSettings();
    expect(calls.mediaCommand).toHaveBeenCalledWith('next');
    expect(calls.startKeepAlive).toHaveBeenCalledWith('PineTime');
    expect(calls.rejectCall).toHaveBeenCalled();
    await expect(bridge.answerCall()).rejects.toThrow('answer calls first');
  });

  it('gives a refusal with no reason a message of its own', async () => {
    const bridge = createTauriPhoneBridge(fakeDevice({ isDoNotDisturb: { ok: false } }).device);
    await expect(bridge.isDoNotDisturb()).rejects.toThrow('The phone bridge failed.');
  });

  it('names an unnamed place Here', async () => {
    const named = createTauriPhoneBridge(
      fakeDevice({ locate: { ok: true, value: { latitude: 18, longitude: -76.8, name: 'Kingston' } } }).device,
    );
    await expect(named.locate()).resolves.toEqual({ latitude: 18, longitude: -76.8, name: 'Kingston' });
    const unnamed = createTauriPhoneBridge(
      fakeDevice({ locate: { ok: true, value: { latitude: 18, longitude: -76.8, name: '' } } }).device,
    );
    await expect(unnamed.locate()).resolves.toMatchObject({ name: 'Here' });
  });

  it('hands on the events it knows and drops the rest', () => {
    const { device, send, stop } = fakeDevice();
    const bridge = createTauriPhoneBridge(device);
    const events: PhoneEvent[] = [];
    const unsubscribe = bridge.subscribe((event) => events.push(event));
    send({ type: 'call', state: 'incoming', number: '555' });
    send({ type: 'unknown' });
    send({ type: 'media', playing: true, track: 'Sinnerman' });
    unsubscribe();
    expect(events).toEqual([
      { type: 'call', state: 'incoming', number: '555' },
      { type: 'media', playing: true, track: 'Sinnerman' },
    ]);
    expect(stop).toHaveBeenCalled();
  });
});

// ============================================================================
// Intents
// ============================================================================

describe('tauriIntents', () => {
  it('is null without the plugin', () => {
    expect(tauriIntents(null)).toBeNull();
  });

  it('starts each kind of intent through the plugin', async () => {
    const { device, calls } = fakeDevice({ sendBroadcast: { ok: false, error: 'refused' } });
    const intents = tauriIntents(device)!;
    await intents.openUri('geo:0,0');
    await intents.startActivity('org.example', 'org.example.Main', null, { a: 'b' });
    expect(calls.openUri).toHaveBeenCalledWith('geo:0,0');
    expect(calls.startActivity).toHaveBeenCalledWith('org.example', 'org.example.Main', null, { a: 'b' });
    await expect(intents.sendBroadcast('org.example.PING', null, {})).rejects.toThrow('refused');
  });
});
