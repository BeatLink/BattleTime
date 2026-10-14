import { describe, expect, it, jest } from '@jest/globals';

import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { PackagesClient } from '@/protocol/packages';
import { testImage } from '@/test-support/app-images';

import type { WatchSession } from './ble';
import { apps, parseRunArguments } from './commands/watch';

// What the apps command needs from a session: the file service and the packages client on one watch.
class FakeSession extends MockLegacyDfuTarget {
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'neotime', true);
  }
  private filesOpened: Promise<FileService> | null = null;
  private packagesOpened: Promise<PackagesClient> | null = null;
  files() {
    this.filesOpened ??= FileService.open(this);
    return this.filesOpened;
  }
  packages() {
    this.packagesOpened ??= PackagesClient.open(this);
    return this.packagesOpened;
  }
}

function imageFile(bytes: Uint8Array): string {
  const path = join(mkdtempSync(join(tmpdir(), 'neotime-')), 'image.ntapp');
  writeFileSync(path, bytes);
  return path;
}

describe('the apps command', () => {
  it('installs an image, lists it, hides, shows and removes it', async () => {
    const session = new FakeSession();
    const said: string[] = [];
    const log = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
      said.push(String(text).trimEnd());
      return true;
    });
    const run = (...args: string[]) => apps(session as unknown as WatchSession, args);

    await run('install', imageFile(testImage('timer', 'Timer')));
    await run('hide', 'timer');
    await run('list');
    await run('show', 'timer');
    await run('remove', 'timer');
    await run('list');
    log.mockRestore();

    expect(said.join('\n')).toContain('Installed Timer as timer.');
    expect(said.some((line) => /^timer\s+app\s+hidden\s+[0-9.]+ kB\s+Timer$/.test(line))).toBe(true);
    expect(said).toContain('Each app may use up to 33.0 kB while it is open.');
    expect(said.some((line) => /^Showed timer; it needs [0-9.]+ kB\.$/.test(line))).toBe(true);
    expect(said).toContain('Removed timer.');
    await expect(run('install')).rejects.toThrow('Give the compiled');
    await expect(run('frobnicate')).rejects.toThrow('Unknown apps action');
  });

  it('says when the watch refuses to show or hide an app', async () => {
    const session = new FakeSession();
    const log = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const run = (...args: string[]) => apps(session as unknown as WatchSession, args);
    await run('install', imageFile(testImage('launcher', 'Apps')));
    const small = testImage('huge', 'Huge');
    const huge = new Uint8Array(small.length + 30000);
    huge.set(small);
    session.fileService.files.set('/apps/huge/app.ntapp', huge);
    session.fileService.files.set('/apps/huge/disabled', new Uint8Array(0));

    await expect(run('hide', 'launcher')).rejects.toThrow('The watch refused to hide launcher: the launcher stays on.');
    await expect(run('show', 'huge')).rejects.toThrow(
      /^The watch refused to show huge: it needs more memory than an app can have \(needs [0-9.]+ kB of 33\.0 kB\)\.$/,
    );
    await expect(run('show', 'nothing')).rejects.toThrow('no such app');
    log.mockRestore();
  });

  it('runs an image as a temporary app with its data, reusing the cached image, then stops and forgets it', async () => {
    const session = new FakeSession();
    const said: string[] = [];
    const log = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
      said.push(String(text).trimEnd());
      return true;
    });
    const run = (...args: string[]) => apps(session as unknown as WatchSession, args);
    const image = imageFile(testImage('prompt', 'Prompt'));
    const dataFile = imageFile(Uint8Array.from([9, 8]));

    await run('run', image, '--text', 'text=Lunch?', '--data', `raw=${dataFile}`);
    await run('run', image, '--text', 'text=Again?');
    await run('stop');
    await run('forget', 'prompt');
    await run('forget');
    log.mockRestore();

    const output = said.join('\n');
    expect(output).toContain("Sent Prompt's image to the watch.");
    expect(output).toContain("The watch already had Prompt's image.");
    expect(said.some((line) => /^Opened Prompt as temporary app prompt; it needs [0-9.]+ kB of 33\.0 kB\.$/.test(line))).toBe(true);
    expect(output).toContain('Closed the temporary app, if one was open.');
    expect(output).toContain('Deleted temporary app prompt.');
    expect(output).toContain('Deleted every temporary app.');
    expect(session.fileService.files.has('/temp/prompt/app.ntapp')).toBe(false);
  });

  it('prints what a temporary app sends while it listens, then closes the app', async () => {
    const session = new FakeSession();
    let onLine: (line: string) => void = () => {};
    const listening = Object.assign(session, { listen: (handler: (line: string) => void) => (onLine = handler) });
    const said: string[] = [];
    const log = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
      said.push(String(text).trimEnd());
      return true;
    });
    const stop = jest.spyOn(PackagesClient.prototype, 'stop');
    const image = imageFile(testImage('touchtest', 'Touch test'));

    await apps(listening as unknown as WatchSession, ['run', image, '--listen'], async () => {
      onLine('{"t":"app","ch":"touchtest","d":{"e":3,"x":120,"y":60,"t":5}}\n');
      onLine('{"t":"notify","id":1}\n');
    });
    log.mockRestore();

    expect(said).toContain('{"t":"app","ch":"touchtest","d":{"e":3,"x":120,"y":60,"t":5}}');
    expect(said.some((line) => line.includes('notify'))).toBe(false);
    expect(said).toContain('Closed Touch test.');
    expect(stop).toHaveBeenCalled();
    stop.mockRestore();
  });

  it('says when the watch refuses to run a temporary app', async () => {
    const session = new FakeSession();
    const log = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const run = (...args: string[]) => apps(session as unknown as WatchSession, args);
    await expect(run('run', imageFile(testImage('huge', 'Huge', 'app', 30000)))).rejects.toThrow(
      /^The watch refused to run image\.ntapp: it needs more memory than an app can have/,
    );
    await expect(run('run', imageFile(testImage('digital', 'Digital', 'face')))).rejects.toThrow('watch face');
    log.mockRestore();
  });

  it('reads the image and the data files apps run is given', () => {
    const read = (path: string) => new TextEncoder().encode(`contents of ${path}`);
    expect(parseRunArguments(['a.ntapp', '--listen'], read)).toEqual({ image: 'a.ntapp', data: {}, listen: true });
    expect(parseRunArguments(['a.ntapp', '--text', 'text=Is it=1?', '--data', 'list=items.txt'], read)).toEqual({
      image: 'a.ntapp',
      data: { text: 'Is it=1?', list: new TextEncoder().encode('contents of items.txt') },
      listen: false,
    });
    expect(parseRunArguments(['--text', 'empty=', 'a.ntapp'], read)).toEqual({
      image: 'a.ntapp',
      data: { empty: '' },
      listen: false,
    });
    expect(() => parseRunArguments([], read)).toThrow('Give the compiled .ntapp image to run.');
    expect(() => parseRunArguments(['a.ntapp', 'b.ntapp'], read)).toThrow('not also b.ntapp');
    expect(() => parseRunArguments(['a.ntapp', '--text', 'nothing'], read)).toThrow('--text takes <name>=<text>.');
    expect(() => parseRunArguments(['a.ntapp', '--data'], read)).toThrow('--data takes <name>=<file>.');
    expect(() => parseRunArguments(['a.ntapp', '--data', '=x'], read)).toThrow('--data takes');
  });

  it('sets up a new watch from the bundled images', async () => {
    const session = new FakeSession();
    const log = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await apps(session as unknown as WatchSession, ['setup']);
    log.mockRestore();
    const installed = await (await session.packages()).apps();
    expect(installed.some((app) => app.kind === 'face')).toBe(true);
  }, 20000);
});
