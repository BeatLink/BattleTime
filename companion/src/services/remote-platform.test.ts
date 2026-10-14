import { afterEach, describe, expect, it, jest } from '@jest/globals';

import type { AndroidIntents } from './remote-actions';
import { desktopCommandRunner, desktopRemoteBridge, remotePlatform, type DesktopRemoteBridge } from './remote-platform';

const parts = { media: null, isPlaying: () => false, findPhone: () => {} };

afterEach(() => {
  delete (globalThis as { neotime?: unknown }).neotime;
});

describe('shell commands in the desktop app', () => {
  it('runs a command over the bridge and hands back how it ended', async () => {
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

  it('finds the bridge the app exposes, and none in a browser', async () => {
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
    await device.web({ method: 'GET', url: 'https://example.com', access: {}, timeoutMs: 1000 });
    expect(fetch).toHaveBeenCalledWith('https://example.com', expect.objectContaining({ method: 'GET' }));
    expect(remotePlatform(parts).android).toBeNull();
  });

  it("uses the global fetch when none is given", async () => {
    const original = globalThis.fetch;
    const global = jest.fn(async () => new Response('ok')) as unknown as typeof globalThis.fetch;
    globalThis.fetch = global;
    try {
      await remotePlatform(parts).web({ method: 'POST', url: 'https://example.com', access: {}, timeoutMs: 1000 });
      expect(global).toHaveBeenCalledWith('https://example.com', expect.objectContaining({ method: 'POST' }));
    } finally {
      globalThis.fetch = original;
    }
  });
});
