import { describe, expect, it, jest } from '@jest/globals';

import { SURVIVAL_GUIDES } from '@/bundles/survival-guides';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { readSurvivalList } from '@/protocol/survival';

import type { WatchSession } from './ble';
import { parseSurvivalArguments, survival } from './commands/survival';

// What the survival command needs from a session: the file service on one watch.
class FakeSession extends MockLegacyDfuTarget {
  constructor() {
    super('AA:BB:CC:DD:EE:00', 'battletime', true);
  }
  private filesOpened: Promise<FileService> | null = null;
  files() {
    this.filesOpened ??= FileService.open(this);
    return this.filesOpened;
  }
}

// Runs the command with the arguments given and returns what it printed.
async function run(session: FakeSession, ...args: string[]): Promise<string[]> {
  const said: string[] = [];
  const out = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  try {
    await survival(session as unknown as WatchSession, args);
  } finally {
    out.mockRestore();
  }
  return said;
}

describe('parseSurvivalArguments', () => {
  it('lists with no arguments or list', () => {
    expect(parseSurvivalArguments([])).toEqual({ kind: 'list' });
    expect(parseSurvivalArguments(['list'])).toEqual({ kind: 'list' });
  });

  it('reads the guides to set, add or remove, with all for every one', () => {
    expect(parseSurvivalArguments(['add', 'cpr', 'water'])).toEqual({ kind: 'add', ids: ['cpr', 'water'] });
    expect(parseSurvivalArguments(['set', 'all'])).toEqual({
      kind: 'set',
      ids: SURVIVAL_GUIDES.map((guide) => guide.id),
    });
  });

  it('refuses a guide it does not know, naming the ones it does', () => {
    expect(() => parseSurvivalArguments(['add', 'kraken'])).toThrow(/kraken.*cpr/);
    expect(() => parseSurvivalArguments(['remove'])).toThrow('Name the guides');
  });
});

describe('the survival command', () => {
  it('adds and removes guides, ticking the ones on the watch', async () => {
    const session = new FakeSession();

    await run(session, 'add', 'lost', 'cpr');
    const said = await run(session, 'remove', 'lost');

    expect(said).toContain('Wilderness');
    expect(said).toContain('  [x] cpr                  CPR');
    expect(said).toContain('  [ ] lost                 Lost in the wild');
    expect(said.at(-1)).toBe('Sent 0 guides and removed 1.');
    expect(await readSurvivalList(await session.files())).toEqual([{ slot: 0, topic: 0, title: 'CPR' }]);
  });
});
