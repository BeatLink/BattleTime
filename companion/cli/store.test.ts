import { describe, expect, it, jest } from '@jest/globals';
import { zipSync } from 'fflate';

import { existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { utf8Encode } from '@/ble/encoding';
import { testImage } from '@/test-support/app-images';

import { store } from './commands/store';
import { imageToInstall } from './commands/watch';

const RELEASES = 'https://api.github.com/repos/a/b/releases?per_page=20';

const releases = [
  {
    id: 1,
    tag_name: 'v1',
    name: 'Version 1',
    published_at: '2026-10-01T00:00:00Z',
    prerelease: false,
    assets: [{ id: 10, name: 'neotime-store.zip', size: 100, url: 'https://api/assets/10' }],
  },
];

const zip = zipSync({
  'store.json': utf8Encode(JSON.stringify({ format: 1, apps: [{ file: 'dice.ntapp' }], plugins: [{ file: 'b.ntplugin' }] })),
  'dice.ntapp': testImage('dice', 'Dice'),
  'b.ntplugin': zipSync({ 'manifest.json': utf8Encode(JSON.stringify({ id: 'buzz-back', name: 'Buzz back' })) }),
});

const fetcher = (async (url: string) => {
  if (url === RELEASES) return { ok: true, json: async () => releases } as Response;
  if (url === 'https://api/assets/10') return { ok: true, arrayBuffer: async () => zip.buffer } as Response;
  return { ok: false, status: 404 } as Response;
}) as typeof fetch;

function capture(): { said: string[]; stop: () => void } {
  const said: string[] = [];
  const spy = jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    said.push(String(text).trimEnd());
    return true;
  });
  return { said, stop: () => spy.mockRestore() };
}

describe('the store command', () => {
  it('lists the bundled apps and faces', async () => {
    const { said, stop } = capture();
    await store([]);
    stop();
    expect(said.some((line) => /^launcher\s+app\s+[0-9.]+ kB\s+/.test(line))).toBe(true);
    expect(said.some((line) => /^digital\s+face\s+/.test(line))).toBe(true);
  });

  it('lists releases and fetches one into a folder', async () => {
    const { said, stop } = capture();
    const folder = join(mkdtempSync(join(tmpdir(), 'neotime-store-')), 'out');
    await store(['releases', 'a/b'], { fetcher });
    await store(['fetch', 'a/b', '1', folder], { fetcher });
    stop();
    expect(said).toContain(' 1  2026-10-01  Version 1');
    expect(existsSync(join(folder, 'dice.ntapp'))).toBe(true);
    expect(existsSync(join(folder, 'buzz-back.ntplugin'))).toBe(true);
    expect(said.at(-1)).toBe(`Fetched 2 from Version 1 into ${folder}.`);
    await expect(store(['fetch', 'a/b', '5'], { fetcher })).rejects.toThrow('no release numbered 5');
    await expect(store(['frobnicate'])).rejects.toThrow('Unknown store action');
  });

  it('installs a bundled app by id when no such file exists', () => {
    expect(imageToInstall('launcher').length).toBeGreaterThan(0);
    expect(() => imageToInstall('no-such-app')).toThrow('neither a file nor an app');
  });
});
