import { describe, expect, it, jest } from '@jest/globals';
import { strToU8, zipSync } from 'fflate';

import { checkRepo, downloadFirmware, firmwareFromDownload, listFirmwareBuilds, type FirmwareBuild } from './github-firmware';

type Call = { url: string; headers?: HeadersInit };

// A fetch that answers from a table of exact URLs, records each call, and gives 404 for anything else.
function routes(table: Record<string, unknown>) {
  const calls: Call[] = [];
  const fetcher = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, headers: init?.headers });
    if (!(url in table)) {
      return new Response('not found', { status: 404 });
    }
    const body = table[url];
    return body instanceof Uint8Array ? new Response(body.slice().buffer) : new Response(JSON.stringify(body));
  }) as typeof fetch;
  return { fetcher, calls };
}

const RELEASES = 'https://api.github.com/repos/BeatLink/BattleTime/releases?per_page=10';
const ARTIFACTS = 'https://api.github.com/repos/BeatLink/BattleTime/actions/artifacts?per_page=50';

const firmwareZip = zipSync({
  'manifest.json': strToU8(JSON.stringify({ manifest: { application: { bin_file: 'a.bin', dat_file: 'a.dat' } } })),
  'a.bin': new Uint8Array([1, 2, 3]),
  'a.dat': new Uint8Array([4]),
});

function build(overrides: Partial<FirmwareBuild>): FirmwareBuild {
  return {
    id: 'x',
    source: 'release',
    title: 'v1',
    detail: 'firmware.zip',
    date: '',
    sizeBytes: 0,
    url: 'https://dl/fw.zip',
    needsToken: false,
    ...overrides,
  };
}

describe('checkRepo', () => {
  it('trims an owner/name and rejects anything else', () => {
    expect(checkRepo('  BeatLink/BattleTime ')).toBe('BeatLink/BattleTime');
    expect(() => checkRepo('BeatLink')).toThrow('owner/name');
    expect(() => checkRepo('a/b/c')).toThrow('owner/name');
  });
});

describe('listFirmwareBuilds', () => {
  it('takes board-named and reloader assets, and marks pre-releases', async () => {
    const { fetcher } = routes({
      [RELEASES]: [
        {
          id: 1,
          tag_name: 'v0.6-rc1',
          name: 'Release candidate',
          published_at: '2026-03-01T00:00:00Z',
          prerelease: true,
          assets: [
            { id: 11, name: 'PineTime-Firmware.zip', size: 10, browser_download_url: 'https://dl/a' },
            { id: 12, name: 'reloader-from-wasp-os.zip', size: 20, browser_download_url: 'https://dl/b' },
            { id: 13, name: 'p8-firmware.zip', size: 30, browser_download_url: 'https://dl/c' },
          ],
        },
      ],
      [ARTIFACTS]: { artifacts: [] },
    });
    const builds = await listFirmwareBuilds('BeatLink/BattleTime', 'pinetime', { fetcher });
    expect(builds.map((entry) => [entry.title, entry.detail])).toEqual([
      ['Release candidate (pre-release)', 'PineTime-Firmware.zip'],
      ['Release candidate (pre-release)', 'reloader-from-wasp-os.zip'],
    ]);
    expect(builds[0]).toMatchObject({ id: 'release-11', sizeBytes: 10, needsToken: false });
  });

  it('names an artifact without run details by the commit in its name, and skips other boards', async () => {
    const { fetcher } = routes({
      [RELEASES]: [],
      [ARTIFACTS]: {
        artifacts: [
          {
            id: 5,
            name: 'pinetime-abcdef123456',
            size_in_bytes: 1,
            expired: false,
            created_at: '2026-01-01T00:00:00Z',
            archive_download_url: 'https://api/5',
          },
          {
            id: 6,
            name: 'p8-abcdef123456',
            size_in_bytes: 1,
            expired: false,
            created_at: '2026-01-02T00:00:00Z',
            archive_download_url: 'https://api/6',
          },
        ],
      },
    });
    const builds = await listFirmwareBuilds('BeatLink/BattleTime', 'pinetime', { fetcher });
    expect(builds).toHaveLength(1);
    expect(builds[0]).toMatchObject({ id: 'actions-5', title: 'abcdef1', source: 'actions', needsToken: true });
  });

  it('sends the token to GitHub when there is one', async () => {
    const { fetcher, calls } = routes({ [RELEASES]: [], [ARTIFACTS]: { artifacts: [] } });
    await listFirmwareBuilds('BeatLink/BattleTime', 'pinetime', { fetcher, token: 'secret' });
    expect(calls[0].headers).toEqual({ Accept: 'application/vnd.github+json', Authorization: 'Bearer secret' });
    await listFirmwareBuilds('BeatLink/BattleTime', 'pinetime', { fetcher });
    expect(calls[2].headers).toEqual({ Accept: 'application/vnd.github+json' });
  });

  it('reports the status and path GitHub refused', async () => {
    const { fetcher } = routes({});
    await expect(listFirmwareBuilds('BeatLink/BattleTime', 'pinetime', { fetcher })).rejects.toThrow(
      'GitHub answered 404 for /repos/BeatLink/BattleTime/releases?per_page=10.',
    );
  });
});

describe('downloading firmware', () => {
  it('fetches a release asset without GitHub headers', async () => {
    const { fetcher, calls } = routes({ 'https://dl/fw.zip': firmwareZip });
    const firmware = await downloadFirmware(build({}), { fetcher, token: 'unused' });
    expect(firmware.name).toBe('firmware.zip');
    expect(calls[0].headers).toEqual({});
  });

  it('sends the token with an Actions artifact', async () => {
    const artifact = zipSync({ 'out/firmware.zip': firmwareZip });
    const { fetcher, calls } = routes({ 'https://api/9': artifact });
    const firmware = await downloadFirmware(
      build({ source: 'actions', needsToken: true, url: 'https://api/9', title: 'master at 1234567' }),
      { fetcher, token: 'secret' },
    );
    expect(firmware.name).toBe('master at 1234567 firmware.zip');
    expect(calls[0].headers).toMatchObject({ Authorization: 'Bearer secret' });
  });

  it('reports a failed download with its status', async () => {
    const { fetcher } = routes({});
    await expect(downloadFirmware(build({}), { fetcher })).rejects.toThrow('Downloading the build failed (404).');
  });

  it('refuses an artifact that holds no firmware zip', () => {
    const artifact = zipSync({ 'bootloader.bin': strToU8(':00') });
    expect(() => firmwareFromDownload(artifact, build({ source: 'actions' }))).toThrow('The build has no firmware.zip.');
  });
});

describe('without options given', () => {
  it('uses the global fetch and no token', async () => {
    const { fetcher, calls } = routes({ [RELEASES]: [], [ARTIFACTS]: { artifacts: [] }, 'https://dl/fw.zip': firmwareZip });
    const spy = jest.spyOn(globalThis, 'fetch').mockImplementation(fetcher);
    try {
      await expect(listFirmwareBuilds('BeatLink/BattleTime', 'pinetime')).resolves.toEqual([]);
      await expect(downloadFirmware(build({}))).resolves.toMatchObject({ name: 'firmware.zip' });
      expect(calls.map((call) => call.url)).toEqual([RELEASES, ARTIFACTS, 'https://dl/fw.zip']);
    } finally {
      spy.mockRestore();
    }
  });
});
