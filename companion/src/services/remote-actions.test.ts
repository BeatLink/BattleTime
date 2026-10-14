import { describe, expect, it, jest } from '@jest/globals';

import { decodeWatchLine, type WatchToPhoneMessage } from '@/protocol/gadgetbridge';
import type { RemoteAction, RemoteButton } from '@/protocol/remote';

import {
  buttonName,
  describeAction,
  handleRemoteTap,
  HTTP_TIMEOUT_MS,
  runRemoteAction,
  unavailableReason,
  type AndroidIntents,
  type CommandResult,
  type RemotePlatform,
} from './remote-actions';
import { fetchRequester, type WebRequest } from './web-request';

// ============================================================================
// A device with every part faked
// ============================================================================

function fakeResponse(status: number, text: string): Response {
  return new Response(text, { status });
}

function platform(overrides: Partial<RemotePlatform> = {}) {
  const fetch = jest.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => fakeResponse(200, 'fine'));
  const runCommand = jest.fn(async (_command: string): Promise<CommandResult> => ({ exitCode: 0, output: 'done\n', timedOut: false }));
  const android: AndroidIntents = {
    openUri: jest.fn(async (_uri: string) => {}),
    startActivity: jest.fn(async () => {}),
    sendBroadcast: jest.fn(async () => {}),
  };
  const media = jest.fn(async (_command: string) => {});
  const findPhone = jest.fn();
  let playing = false;
  const device: RemotePlatform = {
    web: fetchRequester(fetch as unknown as typeof globalThis.fetch),
    runCommand,
    android,
    media,
    isPlaying: () => playing,
    findPhone,
    ...overrides,
  };
  return { device, fetch, runCommand, android, media, findPhone, setPlaying: (value: boolean) => (playing = value) };
}

const BARE: Partial<RemotePlatform> = { runCommand: null, android: null, media: null };

describe('web request actions', () => {
  it('sends the method, headers and body, and reports the status and answer', async () => {
    const { device, fetch } = platform();
    const outcome = await runRemoteAction(
      { kind: 'http', method: 'POST', url: 'https://example.com/hook', headers: { Authorization: 'Bearer x' }, body: '{"on":true}' },
      device,
    );
    expect(outcome).toEqual({ ok: true, message: 'HTTP 200: fine' });
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('https://example.com/hook');
    expect(init).toMatchObject({ method: 'POST', headers: { Authorization: 'Bearer x' }, body: '{"on":true}' });
  });

  it('sends no body with a GET and counts an error status as a failure', async () => {
    const { device, fetch } = platform();
    fetch.mockResolvedValueOnce(fakeResponse(404, ''));
    const outcome = await runRemoteAction({ kind: 'http', method: 'GET', url: 'https://example.com', body: 'x' }, device);
    expect(outcome).toEqual({ ok: false, message: 'HTTP 404' });
    expect(fetch.mock.calls[0][1]?.body).toBeUndefined();
  });

  it("passes the button's login and certificate choice to the device's requests", async () => {
    const requests: WebRequest[] = [];
    const { device } = platform({
      web: async (request) => {
        requests.push(request);
        return { status: 204, statusText: 'No Content', body: new Uint8Array() };
      },
    });
    const action: RemoteAction = {
      kind: 'http',
      method: 'GET',
      url: 'https://nas.local/hook',
      username: 'me',
      password: 'secret',
      certificate: 'pinned',
      certificateSha256: 'ab'.repeat(32),
    };

    await expect(runRemoteAction(action, device)).resolves.toEqual({ ok: true, message: 'HTTP 204' });
    expect(requests[0]).toMatchObject({ url: 'https://nas.local/hook', access: action, timeoutMs: HTTP_TIMEOUT_MS });
  });

  it('shortens a long answer for the log', async () => {
    const { device, fetch } = platform();
    fetch.mockResolvedValueOnce(fakeResponse(200, 'a'.repeat(500)));
    const outcome = await runRemoteAction({ kind: 'http', method: 'GET', url: 'https://example.com' }, device);
    expect(outcome.message.length).toBeLessThan(140);
    expect(outcome.message.endsWith('…')).toBe(true);
  });

  it('reports a network failure, and gives up on a request that never answers', async () => {
    const { device, fetch } = platform();
    fetch.mockRejectedValueOnce(new Error('Network request failed'));
    await expect(runRemoteAction({ kind: 'http', method: 'GET', url: 'https://example.com' }, device)).resolves.toEqual({
      ok: false,
      message: 'Network request failed',
    });

    jest.useFakeTimers();
    try {
      fetch.mockImplementationOnce(
        (_input, init) =>
          new Promise((_resolve, reject) => init?.signal?.addEventListener('abort', () => reject(new Error('aborted')))),
      );
      const pending = runRemoteAction({ kind: 'http', method: 'GET', url: 'https://example.com' }, device);
      jest.advanceTimersByTime(HTTP_TIMEOUT_MS);
      await expect(pending).resolves.toEqual({ ok: false, message: 'No answer within 15 s.' });
    } finally {
      jest.useRealTimers();
    }
  });
});

describe('shell command actions', () => {
  it('runs the command through the desktop bridge and reports its exit code and output', async () => {
    const { device, runCommand } = platform();
    await expect(runRemoteAction({ kind: 'command', command: 'echo done' }, device)).resolves.toEqual({
      ok: true,
      message: 'Exit 0: done',
    });
    expect(runCommand).toHaveBeenCalledWith('echo done');
  });

  it('counts a non-zero exit, a killed command and a timeout as failures', async () => {
    const { device, runCommand } = platform();
    runCommand.mockResolvedValueOnce({ exitCode: 2, output: '', timedOut: false });
    await expect(runRemoteAction({ kind: 'command', command: 'false' }, device)).resolves.toEqual({ ok: false, message: 'Exit 2' });
    runCommand.mockResolvedValueOnce({ exitCode: null, output: 'bye', timedOut: false });
    await expect(runRemoteAction({ kind: 'command', command: 'x' }, device)).resolves.toEqual({ ok: false, message: 'Exit killed: bye' });
    runCommand.mockResolvedValueOnce({ exitCode: null, output: 'partial', timedOut: true });
    await expect(runRemoteAction({ kind: 'command', command: 'sleep 99' }, device)).resolves.toEqual({
      ok: false,
      message: 'Stopped after running too long: partial',
    });
    runCommand.mockResolvedValueOnce({ exitCode: null, output: '', timedOut: true });
    await expect(runRemoteAction({ kind: 'command', command: 'sleep 99' }, device)).resolves.toEqual({
      ok: false,
      message: 'Stopped after running too long',
    });
  });

  it('reports a bridge that fails', async () => {
    const { device, runCommand } = platform();
    runCommand.mockRejectedValueOnce('bridge gone');
    await expect(runRemoteAction({ kind: 'command', command: 'x' }, device)).resolves.toEqual({ ok: false, message: 'bridge gone' });
  });
});

describe('Android intent actions', () => {
  it('opens a link, starts an activity and sends a broadcast through the bridge', async () => {
    const { device, android } = platform();
    await expect(
      runRemoteAction({ kind: 'android', intent: { type: 'deeplink', uri: 'geo:0,0?q=coffee' } }, device),
    ).resolves.toEqual({ ok: true, message: 'Open geo:0,0?q=coffee: done.' });
    expect(android.openUri).toHaveBeenCalledWith('geo:0,0?q=coffee');

    await runRemoteAction(
      { kind: 'android', intent: { type: 'activity', packageName: 'org.example', className: 'org.example.Main', extras: { a: '1' } } },
      device,
    );
    expect(android.startActivity).toHaveBeenCalledWith('org.example', 'org.example.Main', null, { a: '1' });
    await runRemoteAction(
      { kind: 'android', intent: { type: 'activity', packageName: 'p', className: 'c', action: 'GO' } },
      device,
    );
    expect(android.startActivity).toHaveBeenLastCalledWith('p', 'c', 'GO', {});

    await runRemoteAction({ kind: 'android', intent: { type: 'broadcast', action: 'org.example.PING' } }, device);
    expect(android.sendBroadcast).toHaveBeenCalledWith('org.example.PING', null, {});
    await runRemoteAction(
      { kind: 'android', intent: { type: 'broadcast', action: 'A', packageName: 'p', extras: { b: '2' } } },
      device,
    );
    expect(android.sendBroadcast).toHaveBeenLastCalledWith('A', 'p', { b: '2' });
  });

  it('reports what the bridge refuses', async () => {
    const { device, android } = platform();
    jest.mocked(android.openUri).mockRejectedValueOnce(new Error('No app on this phone handles that'));
    await expect(runRemoteAction({ kind: 'android', intent: { type: 'deeplink', uri: 'x:y' } }, device)).resolves.toEqual({
      ok: false,
      message: 'No app on this phone handles that',
    });
  });
});

describe('ready-made actions', () => {
  it('sends media keys, and turns play/pause into whichever fits', async () => {
    const { device, media, setPlaying } = platform();
    await expect(runRemoteAction({ kind: 'builtin', name: 'next' }, device)).resolves.toEqual({
      ok: true,
      message: 'Sent next to the media player.',
    });
    await runRemoteAction({ kind: 'builtin', name: 'playpause' }, device);
    setPlaying(true);
    await runRemoteAction({ kind: 'builtin', name: 'playpause' }, device);
    await runRemoteAction({ kind: 'builtin', name: 'volumedown' }, device);
    expect(media.mock.calls.map(([command]) => command)).toEqual(['next', 'play', 'pause', 'volumedown']);
  });

  it('shows the find phone card, even with no media control', async () => {
    const { device, findPhone } = platform(BARE);
    await expect(runRemoteAction({ kind: 'builtin', name: 'findphone' }, device)).resolves.toMatchObject({ ok: true });
    expect(findPhone).toHaveBeenCalled();
  });
});

describe('actions this device cannot run', () => {
  const actions: RemoteAction[] = [
    { kind: 'command', command: 'ls' },
    { kind: 'android', intent: { type: 'deeplink', uri: 'x:y' } },
    { kind: 'builtin', name: 'play' },
  ];

  it('are named unavailable with the reason, and skipped without touching the platform', async () => {
    const { device } = platform(BARE);
    expect(actions.map((action) => unavailableReason(action, device))).toEqual([
      'Shell commands run only in the desktop app.',
      'Android intents run only in the Android app.',
      'Media control needs the Android app or the desktop app.',
    ]);
    for (const action of actions) {
      const outcome = await runRemoteAction(action, device);
      expect(outcome.ok).toBe(false);
      expect(outcome.message).toMatch(/^Skipped: /);
    }
    expect(unavailableReason({ kind: 'http', method: 'GET', url: 'https://x' }, device)).toBeNull();
  });

  it('are all available where every part is there', () => {
    const { device } = platform();
    expect(actions.every((action) => unavailableReason(action, device) === null)).toBe(true);
  });
});

describe('describing buttons', () => {
  it('names each kind of action in a short line', () => {
    expect(describeAction({ kind: 'http', method: 'PUT', url: 'https://x' })).toBe('PUT https://x');
    expect(describeAction({ kind: 'command', command: 'ls' })).toBe('Run ls');
    expect(describeAction({ kind: 'android', intent: { type: 'activity', packageName: 'p', className: 'c' } })).toBe('Start p/c');
    expect(describeAction({ kind: 'android', intent: { type: 'broadcast', action: 'A' } })).toBe('Broadcast A');
    expect(describeAction({ kind: 'builtin', name: 'findphone' })).toBe('Find this device');
  });

  it('uses the label, or the action without one', () => {
    expect(buttonName({ icon: 'play', label: ' Lights ', action: { kind: 'builtin', name: 'play' } })).toBe('Lights');
    expect(buttonName({ icon: 'play', label: '', action: { kind: 'builtin', name: 'play' } })).toBe('Play');
  });
});

describe('taps from the watch', () => {
  const BUTTONS: RemoteButton[] = [
    { icon: 'play', label: 'Music', action: { kind: 'builtin', name: 'playpause' } },
    { icon: 'flashlight', label: 'Lights', action: { kind: 'http', method: 'POST', url: 'https://example.com/lights' } },
    { icon: 'settings', label: 'Script', action: { kind: 'command', command: 'echo hi' } },
  ];
  const message = (line: string) => decodeWatchLine(line) as WatchToPhoneMessage;
  const now = () => 1000;

  it("runs the stored button the tap names and logs how it went", async () => {
    const { device, fetch } = platform();
    const entry = await handleRemoteTap(message('{"t":"app","ch":"remote","d":{"button":1}}'), BUTTONS, device, now);
    expect(entry).toEqual({ at: 1000, button: 1, label: 'Lights', source: 'watch', ok: true, message: 'HTTP 200: fine' });
    expect(fetch.mock.calls[0][0]).toBe('https://example.com/lights');
  });

  it('never takes a URL or command from the message, only the index', async () => {
    const { device, fetch, runCommand } = platform();
    await handleRemoteTap(
      message('{"t":"app","ch":"remote","d":{"button":1,"url":"https://evil.example","command":"rm -rf ~"}}'),
      BUTTONS,
      device,
      now,
    );
    expect(fetch.mock.calls[0][0]).toBe('https://example.com/lights');
    expect(runCommand).not.toHaveBeenCalled();
  });

  it('ignores an index past the last button, and logs it', async () => {
    const { device, fetch, media, runCommand } = platform();
    const entry = await handleRemoteTap(message('{"t":"app","ch":"remote","d":{"button":3}}'), BUTTONS, device, now);
    expect(entry).toMatchObject({ button: 3, ok: false, message: 'Ignored: there is no button 4.' });
    expect(fetch).not.toHaveBeenCalled();
    expect(media).not.toHaveBeenCalled();
    expect(runCommand).not.toHaveBeenCalled();
  });

  it('gives nothing for malformed taps and other messages', async () => {
    const { device } = platform();
    for (const line of [
      '{"t":"app","ch":"remote","d":{"button":"0"}}',
      '{"t":"app","ch":"remote","d":{}}',
      '{"t":"app","ch":"timer","d":{"button":0}}',
      '{"t":"music","n":"next"}',
    ]) {
      await expect(handleRemoteTap(message(line), BUTTONS, device, now)).resolves.toBeNull();
    }
  });

  it('skips a button this device cannot run, with the reason', async () => {
    const { device } = platform(BARE);
    const entry = await handleRemoteTap(message('{"t":"app","ch":"remote","d":{"button":2}}'), BUTTONS, device);
    expect(entry).toMatchObject({ ok: false, message: 'Skipped: Shell commands run only in the desktop app.' });
  });
});
