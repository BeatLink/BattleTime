import { describe, expect, it, jest } from '@jest/globals';

import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { PackagesClient } from '@/protocol/packages';
import { testImage } from '@/test-support/app-images';

import type { WatchSession } from './ble';
import { apps } from './commands/watch';

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

  it('sets up a new watch from the bundled images', async () => {
    const session = new FakeSession();
    const log = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await apps(session as unknown as WatchSession, ['setup']);
    log.mockRestore();
    const installed = await (await session.packages()).apps();
    expect(installed.some((app) => app.kind === 'face')).toBe(true);
  }, 20000);
});
