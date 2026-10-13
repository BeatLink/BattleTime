import { afterEach, beforeEach, describe, expect, it, jest } from '@jest/globals';

import type { AndroidIntents } from './remote-actions';
import { desktopCommandRunner, desktopRemoteBridge, remotePlatform, type DesktopRemoteBridge } from './remote-platform';

const mockRequireOptionalNativeModule = jest.fn<(name: string) => unknown>();

jest.mock('expo', () => ({
  requireOptionalNativeModule: (name: string) => mockRequireOptionalNativeModule(name),
}));

const parts = { media: null, isPlaying: () => false, findPhone: () => {} };

beforeEach(() => {
  mockRequireOptionalNativeModule.mockReset();
});

afterEach(() => {
  delete (globalThis as { neotime?: unknown }).neotime;
});

describe('shell commands in the desktop app', () => {
  it('runs a command over the preload bridge and hands back how it ended', async () => {
    const bridge: DesktopRemoteBridge = {
      runCommand: jest.fn(async () => ({ ok: true, value: { exitCode: 0, output: 'hi', timedOut: false } })),
    };
    await expect(desktopCommandRunner(bridge)('echo hi')).resolves.toEqual({ exitCode: 0, output: 'hi', timedOut: false });
    expect(bridge.runCommand).toHaveBeenCalledWith('echo hi');
  });

  it('throws the bridge error', async () => {
    const failing: DesktopRemoteBridge = { runCommand: async () => ({ ok: false, error: 'Give a command to run.' }) };
    await expect(desktopCommandRunner(failing)('')).rejects.toThrow('Give a command to run.');
    const empty: DesktopRemoteBridge = { runCommand: async () => ({ ok: false }) };
    await expect(desktopCommandRunner(empty)('x')).rejects.toThrow('could not run the command');
  });

  it('finds the bridge the preload script exposes, and none in a browser', async () => {
    expect(desktopRemoteBridge()).toBeNull();
    expect(remotePlatform(parts).runCommand).toBeNull();
    const remote: DesktopRemoteBridge = { runCommand: async () => ({ ok: true, value: { exitCode: 3, output: '', timedOut: false } }) };
    (globalThis as { neotime?: unknown }).neotime = { remote };
    expect(desktopRemoteBridge()).toBe(remote);
    await expect(remotePlatform(parts).runCommand!('false')).resolves.toMatchObject({ exitCode: 3 });
  });
});

describe('the device for remote buttons', () => {
  it('takes the parts it is given and has no Android intents off Android', async () => {
    const android: AndroidIntents = { openUri: async () => {}, startActivity: async () => {}, sendBroadcast: async () => {} };
    const fetch = jest.fn(async () => new Response('ok')) as unknown as typeof globalThis.fetch;
    const device = remotePlatform({ ...parts, desktop: null, android, fetch });
    expect(device.android).toBe(android);
    expect(device.runCommand).toBeNull();
    await device.fetch('https://example.com');
    expect(fetch).toHaveBeenCalledWith('https://example.com');
    expect(remotePlatform(parts).android).toBeNull();
  });

  it("uses the global fetch when none is given", async () => {
    const original = globalThis.fetch;
    const global = jest.fn(async () => new Response('ok')) as unknown as typeof globalThis.fetch;
    globalThis.fetch = global;
    try {
      await remotePlatform(parts).fetch('https://example.com', { method: 'POST' });
      expect(global).toHaveBeenCalledWith('https://example.com', { method: 'POST' });
    } finally {
      globalThis.fetch = original;
    }
  });
});

describe('Android intents', () => {
  function load(native: unknown) {
    mockRequireOptionalNativeModule.mockReturnValue(native);
    let intents: AndroidIntents | null = null;
    jest.isolateModules(() => {
      intents = jest.requireActual<typeof import('./android-intents.android')>('./android-intents.android').androidIntents;
    });
    return intents as AndroidIntents | null;
  }

  it('passes each intent through to the native module', async () => {
    const native = {
      openUri: jest.fn(async (_uri: string) => {}),
      startActivity: jest.fn(async () => {}),
      sendBroadcast: jest.fn(async () => {}),
    };
    const intents = load(native)!;
    await intents.openUri('geo:0,0');
    await intents.startActivity('p', 'c', null, { a: '1' });
    await intents.sendBroadcast('A', 'p', {});
    expect(native.openUri).toHaveBeenCalledWith('geo:0,0');
    expect(native.startActivity).toHaveBeenCalledWith('p', 'c', null, { a: '1' });
    expect(native.sendBroadcast).toHaveBeenCalledWith('A', 'p', {});
    expect(mockRequireOptionalNativeModule).toHaveBeenCalledWith('PhoneBridge');
  });

  it('has none without the module, as in Expo Go, or with a module built before intents', () => {
    expect(load(null)).toBeNull();
    expect(load({ openUri: async () => {} })).toBeNull();
  });
});
