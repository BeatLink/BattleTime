import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { decodeRemoteLayout, REMOTE_BUTTONS_PATH } from '@/protocol/remote';

import type { WatchSession } from './ble';
import { loadRemoteButtons, remote, remoteButtonsPath, terminalPlatform } from './commands/remote';

const BUTTONS = [
  { icon: 'play', label: 'Hello', action: { kind: 'command', command: 'echo hello' } },
  { icon: 'next', label: '', action: { kind: 'builtin', name: 'next' } },
  { icon: 'teapot', label: 'Broken', action: { kind: 'command', command: 'x' } },
];

function buttonsFile(contents: unknown = BUTTONS): string {
  const path = join(mkdtempSync(join(tmpdir(), 'neotime-remote-')), 'remote.json');
  writeFileSync(path, JSON.stringify(contents));
  return path;
}

let printed: string[] = [];
const output = () => printed.join('');

function capture() {
  printed = [];
  jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    printed.push(String(text));
    return true;
  });
}

afterEach(() => {
  jest.restoreAllMocks();
});

describe('the remote command', () => {
  it('lists the buttons in the file, marking what needs the app', async () => {
    capture();
    await remote(null, ['list'], buttonsFile());
    expect(output()).toContain('1. play         Hello  Run echo hello');
    expect(output()).toContain('2. next         Next track  Next track  (needs the app)');
    expect(output()).not.toContain('Broken');
    await remote(null, [], buttonsFile([]));
    expect(output()).toContain('No buttons.');
  });

  it('runs a button here, in the shell', async () => {
    capture();
    await remote(null, ['run', '1'], buttonsFile());
    expect(output()).toContain('Hello: Exit 0: hello');
  });

  it('fails for a skipped action and an unknown button number', async () => {
    capture();
    await expect(remote(null, ['run', '2'], buttonsFile())).rejects.toThrow('The action failed.');
    expect(output()).toContain('Skipped: Media control');
    await expect(remote(null, ['run', '7'], buttonsFile())).rejects.toThrow('from 1 to 2');
    await expect(remote(null, ['run'], buttonsFile())).rejects.toThrow('from 1 to 2');
    await expect(remote(null, ['dance'], buttonsFile())).rejects.toThrow('Unknown remote action: dance');
  });

  it('sends the layout to the watch', async () => {
    capture();
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = FileService.open(watch);
    const session = { deviceId: watch.deviceId, files: () => service } as unknown as WatchSession;
    await remote(session, ['push'], buttonsFile());
    expect(decodeRemoteLayout(await (await service).read(REMOTE_BUTTONS_PATH))).toEqual(['play', 'next']);
    expect(output()).toContain(`Sent 2 buttons to ${REMOTE_BUTTONS_PATH}.`);
  });

  it('runs each button tapped on the watch while listening', async () => {
    capture();
    let onLine: (line: string) => void = () => {};
    const session = { listen: (listener: (line: string) => void) => (onLine = listener) } as unknown as WatchSession;
    const runCommand = jest.fn(async () => ({ exitCode: 0, output: 'ran', timedOut: false }));
    const listening = remote(session, ['listen'], buttonsFile(), terminalPlatform(runCommand));
    await new Promise((resolve) => setTimeout(resolve, 10));
    onLine('{"t":"app","ch":"remote","d":{"button":0,"command":"rm -rf ~"}}');
    onLine('{"t":"app","ch":"remote","d":{"button":5}}');
    onLine('not json');
    await new Promise((resolve) => setTimeout(resolve, 10));
    process.emit('SIGINT');
    await listening;
    expect(runCommand).toHaveBeenCalledWith('echo hello');
    expect(output()).toContain('Hello: Exit 0: ran');
    expect(output()).toContain('Ignored: there is no button 6.');
  });

  it('reads the buttons from the config folder unless given a file, and says when there are none', () => {
    process.env.XDG_CONFIG_HOME = '/somewhere';
    expect(remoteButtonsPath()).toBe('/somewhere/neotime-companion/remote.json');
    delete process.env.XDG_CONFIG_HOME;
    expect(remoteButtonsPath('mine.json')).toBe('mine.json');
    expect(() => loadRemoteButtons('/no/such/remote.json')).toThrow('No remote buttons at /no/such/remote.json');
  });

  it('tells the terminal when a button asks to find this computer', () => {
    capture();
    terminalPlatform().findPhone();
    expect(output()).toContain('looking for this computer');
  });
});
