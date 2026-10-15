import { describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { installImage, PackagesClient } from '@/protocol/packages';
import { QUICK_APPS_PATH } from '@/protocol/quick-apps';
import { testImage } from '@/test-support/app-images';

import type { WatchSession } from './ble';
import { quickApps } from './commands/watch';

// What the quick-apps command needs from a session: the file service and the packages client on one watch.
class FakeSession extends MockLegacyDfuTarget {
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'battletime', true);
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

// Runs the command with the arguments given and returns what it printed.
async function run(session: FakeSession, ...args: string[]): Promise<string[]> {
  const said: string[] = [];
  const log = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  try {
    await quickApps(session as unknown as WatchSession, args);
  } finally {
    log.mockRestore();
  }
  return said;
}

describe('the quick-apps command', () => {
  it('lists the default ring, marking the apps the watch would skip', async () => {
    const session = new FakeSession();
    await installImage(await session.files(), await session.packages(), testImage('steps', 'Steps'));
    const said = await run(session);
    expect(said[0]).toBe(' 1. steps');
    expect(said[1]).toBe(' 2. stopwatch  (not installed, skipped)');
    expect(said).toContain("This is the watch's default ring.");
  });

  it('sets the ring, lists it, and resets it', async () => {
    const session = new FakeSession();
    expect(await run(session, 'set', 'heart', 'steps')).toEqual(['The ring is now heart, steps.']);
    expect(new TextDecoder().decode(session.fileService.files.get(QUICK_APPS_PATH)!)).toBe('heart\nsteps\n');
    const listed = await run(session, 'list');
    expect(listed).toHaveLength(2);
    expect(listed[0]).toContain('heart');
    expect(await run(session, 'reset')).toEqual(['The watch is back on its default ring.']);
    expect(session.fileService.files.has(QUICK_APPS_PATH)).toBe(false);
  });

  it('says so when the ring is empty', async () => {
    const session = new FakeSession();
    session.fileService.files.set(QUICK_APPS_PATH, new Uint8Array(0));
    expect(await run(session)).toEqual(['The ring is empty.']);
  });

  it('refuses a set with no ids and an unknown action', async () => {
    const session = new FakeSession();
    await expect(run(session, 'set')).rejects.toThrow('Give the app ids');
    await expect(run(session, 'shuffle')).rejects.toThrow('not shuffle');
  });
});
