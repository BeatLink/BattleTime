import { describe, expect, it } from '@jest/globals';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

import { ICONS_PATH, readLibraryIcons } from '@/protocol/library-icons';
import { base64ToBytes } from '@/ble/encoding';

import { LIBRARY_ICON_NAMES } from './icon-names';
import { SYSTEM_FILES } from './system-files';

// BattleTime's list of icon names, from BATTLETIME_DIR or a BattleTime checkout beside this one or any folder above it.
function findBattleTimeTypes(): string | null {
  const candidates: string[] = [];
  if (process.env.BATTLETIME_DIR) {
    candidates.push(process.env.BATTLETIME_DIR);
  }
  for (let folder = __dirname; dirname(folder) !== folder; folder = dirname(folder)) {
    candidates.push(join(folder, 'BattleTime'));
  }
  const found = candidates.map((root) => join(root, 'compiler', 'src', 'types.rs')).find((path) => existsSync(path));
  return found ?? null;
}

function readIconNames(types: string): string[] {
  const match = /pub const ICON_NAMES: \[&str; (\d+)\] = \[([^\]]*)\];/.exec(types);
  if (!match) {
    throw new Error('compiler/src/types.rs has no ICON_NAMES list.');
  }
  return [...match[2].matchAll(/"([^"]+)"/g)].map((found) => found[1]);
}

const typesPath = findBattleTimeTypes();
// CI has no BattleTime checkout, so the comparison with it runs only where one is found.
const withBattleTime = typesPath ? it : it.skip;

describe('the library icon names', () => {
  withBattleTime('match ICON_NAMES in BattleTime, in the same order; run npm run import-icon-names if not', () => {
    expect([...LIBRARY_ICON_NAMES]).toEqual(readIconNames(readFileSync(typesPath!, 'utf8')));
  });

  it('name each picture in the bundled icons file once', () => {
    const icons = SYSTEM_FILES.find((file) => file.path === ICONS_PATH)!;
    expect(readLibraryIcons(base64ToBytes(icons.base64))).toHaveLength(LIBRARY_ICON_NAMES.length);
    expect(new Set(LIBRARY_ICON_NAMES).size).toBe(LIBRARY_ICON_NAMES.length);
  });

  it('are read from the declaration the way the import script reads them', () => {
    expect(readIconNames('pub const ICON_NAMES: [&str; 2] = [\n    "alarm",\n    "bell",\n];')).toEqual(['alarm', 'bell']);
    expect(() => readIconNames('nothing here')).toThrow('no ICON_NAMES');
  });
});
