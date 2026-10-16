import { createHash } from 'node:crypto';

import { describe, expect, it } from '@jest/globals';
import { zipSync } from 'fflate';

import { utf8Encode } from '@/ble/encoding';
import { testImage } from '@/test-support/app-images';

import { type StoreEntry } from './app-store';
import {
  fetchOnlinePlugin,
  fetchStoreRelease,
  listOnlinePlugins,
  listStoreReleases,
  offersFromStoreZip,
  withOnlinePlugins,
  type StoreRelease,
} from './github-store';

type Call = { url: string; headers: Record<string, string> };

// Answers each URL with the JSON or bytes given for it, and records what was asked.
function fakeFetch(answers: Record<string, unknown>, calls: Call[] = []): typeof fetch {
  return (async (url: string, init?: { headers?: Record<string, string> }) => {
    calls.push({ url, headers: init?.headers ?? {} });
    const answer = answers[url];
    if (answer === undefined) {
      return { ok: false, status: 404, statusText: 'Not Found', arrayBuffer: async () => new ArrayBuffer(0) } as Response;
    }
    return {
      ok: true,
      status: 200,
      json: async () => answer,
      arrayBuffer: async () => {
        const bytes = answer as Uint8Array;
        return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      },
    } as Response;
  }) as typeof fetch;
}

const RELEASES = 'https://api.github.com/repos/a/b/releases?per_page=20';
const ARTIFACTS = 'https://api.github.com/repos/a/b/actions/artifacts?per_page=50&name=battletime-store';

const releases = [
  {
    id: 1,
    tag_name: 'v1',
    name: 'Version 1',
    published_at: '2026-10-01T00:00:00Z',
    prerelease: false,
    assets: [
      { id: 10, name: 'battletime-store.zip', size: 100, url: 'https://api/assets/10' },
      { id: 11, name: 'dice.ntapp', size: 10, url: 'https://api/assets/11' },
      { id: 12, name: 'firmware.zip', size: 10, url: 'https://api/assets/12' },
    ],
  },
  {
    id: 2,
    tag_name: 'apps-latest',
    name: null,
    published_at: '2026-10-05T00:00:00Z',
    prerelease: true,
    assets: [{ id: 20, name: 'timer.ntapp', size: 10, url: 'https://api/assets/20' }],
  },
  { id: 3, tag_name: 'v0', name: 'Firmware only', published_at: '2026-09-01T00:00:00Z', prerelease: false, assets: [] },
];

const artifacts = {
  artifacts: [
    {
      id: 30,
      name: 'battletime-store',
      size_in_bytes: 300,
      expired: false,
      created_at: '2026-10-10T00:00:00Z',
      archive_download_url: 'https://api/artifacts/30/zip',
      workflow_run: { head_branch: 'master', head_sha: 'abcdef1234' },
    },
    { id: 31, name: 'battletime-store', size_in_bytes: 1, expired: true, created_at: '2026-10-11T00:00:00Z', archive_download_url: '' },
  ],
};

function storeZip(index: unknown | null): Uint8Array {
  const files: Record<string, Uint8Array> = {
    'dice.ntapp': testImage('dice', 'Dice'),
    'buzz-back.ntplugin': zipSync({
      'manifest.json': utf8Encode(JSON.stringify({ id: 'buzz-back', name: 'Buzz back', version: '1.0.0' })),
    }),
    'notes.txt': utf8Encode('not for the store'),
  };
  if (index !== null) files['store.json'] = utf8Encode(JSON.stringify(index));
  return zipSync(files);
}

describe('finding apps on GitHub', () => {
  it('lists releases with store files, newest first, and Actions builds only with a token', async () => {
    const calls: Call[] = [];
    const found = await listStoreReleases('a/b', { fetcher: fakeFetch({ [RELEASES]: releases }, calls) });
    expect(found.map((release) => release.title)).toEqual(['apps-latest (pre-release)', 'Version 1']);
    expect(found[1].bundle?.name).toBe('battletime-store.zip');
    expect(found[1].files.map((file) => file.name)).toEqual(['dice.ntapp']);
    expect(calls.map((call) => call.url)).toEqual([RELEASES]);

    const withToken = await listStoreReleases('a/b', {
      token: 'secret',
      fetcher: fakeFetch({ [RELEASES]: releases, [ARTIFACTS]: artifacts }),
    });
    expect(withToken[0]).toMatchObject({ source: 'actions', title: 'master at abcdef1', needsToken: true });
    expect(withToken).toHaveLength(3);
  });

  it('says what GitHub answered when it refuses, and checks the repository name', async () => {
    await expect(listStoreReleases('a/b', { fetcher: fakeFetch({}) })).rejects.toThrow('404');
    await expect(listStoreReleases('not a repo', { fetcher: fakeFetch({}) })).rejects.toThrow('owner/name');
  });
});

describe('reading a store zip', () => {
  const source = { type: 'github' as const, repo: 'a/b', release: 'v1' };

  it('takes the list and descriptions from store.json', () => {
    const offers = offersFromStoreZip(
      storeZip({
        format: 1,
        apps: [
          { file: 'dice.ntapp', description: 'Rolls dice.' },
          { file: 'missing.ntapp', description: '' },
        ],
        plugins: [{ file: 'buzz-back.ntplugin' }],
      }),
      source,
    );
    expect(offers.map((offer) => [offer.entry.key, offer.entry.description])).toEqual([
      ['watch:dice', 'Rolls dice.'],
      ['plugin:buzz-back', ''],
    ]);
    expect(offers[0].entry.source).toEqual(source);
  });

  it('offers every app and plugin file when there is no store.json', () => {
    const offers = offersFromStoreZip(storeZip(null), source);
    expect(offers.map((offer) => offer.entry.key)).toEqual(['plugin:buzz-back', 'watch:dice']);
  });

  it('refuses a store of another format and a download that is not a zip', () => {
    expect(() => offersFromStoreZip(storeZip({ format: 2 }), source)).toThrow('another version');
    expect(() => offersFromStoreZip(utf8Encode('hello'), source)).toThrow('not a zip');
  });
});

describe('downloading from GitHub', () => {
  const zipRelease: StoreRelease = {
    id: 'release-1',
    source: 'release',
    title: 'Version 1',
    date: '',
    bundle: { name: 'battletime-store.zip', url: 'https://api/assets/10', sizeBytes: 1 },
    files: [],
    needsToken: false,
  };

  it('downloads the store zip as an octet stream', async () => {
    const calls: Call[] = [];
    const offers = await fetchStoreRelease('a/b', zipRelease, {
      fetcher: fakeFetch({ 'https://api/assets/10': storeZip({ format: 1, apps: [{ file: 'dice.ntapp' }] }) }, calls),
    });
    expect(offers.map((offer) => offer.entry.id)).toEqual(['dice']);
    expect(offers[0].entry.source).toEqual({ type: 'github', repo: 'a/b', release: 'Version 1' });
    expect(calls[0].headers.Accept).toBe('application/octet-stream');
  });

  it('downloads each loose file of a release without a store zip', async () => {
    const release = { ...zipRelease, bundle: null, files: [{ name: 'timer.ntapp', url: 'https://api/assets/20', sizeBytes: 1 }] };
    const offers = await fetchStoreRelease('a/b', release, {
      fetcher: fakeFetch({ 'https://api/assets/20': testImage('timer', 'Timer') }),
    });
    expect(offers.map((offer) => offer.entry.id)).toEqual(['timer']);
    await expect(fetchStoreRelease('a/b', release, { fetcher: fakeFetch({}) })).rejects.toThrow('timer.ntapp failed (404)');
  });

  it('downloads through the native requester when given one', async () => {
    const asked: string[] = [];
    const offers = await fetchStoreRelease('a/b', zipRelease, {
      requester: async (request) => {
        asked.push(`${request.method} ${request.url} ${request.headers?.Accept}`);
        return { status: 200, statusText: 'OK', body: storeZip(null) };
      },
    });
    expect(asked).toEqual(['GET https://api/assets/10 application/octet-stream']);
    expect(offers).toHaveLength(2);
    await expect(
      fetchStoreRelease('a/b', zipRelease, {
        requester: async () => ({ status: 302, statusText: 'Found', body: new Uint8Array() }),
      }),
    ).rejects.toThrow('battletime-store.zip failed (302)');
  });

  it('needs a token for an Actions build and sends it', async () => {
    const build = { ...zipRelease, source: 'actions' as const, needsToken: true };
    await expect(fetchStoreRelease('a/b', build)).rejects.toThrow('token');
    const calls: Call[] = [];
    await fetchStoreRelease('a/b', build, {
      token: 'secret',
      fetcher: fakeFetch({ 'https://api/assets/10': storeZip(null) }, calls),
    });
    expect(calls[0].headers.Authorization).toBe('Bearer secret');
  });
});

describe("the repository's plugins online", () => {
  const plugin = zipSync({
    'manifest.json': utf8Encode(JSON.stringify({ id: 'buzz-back', name: 'Buzz back', version: '1.1.0' })),
  });
  const sum = createHash('sha256').update(plugin).digest('hex');
  const buzzBack = {
    id: 'buzz-back',
    name: 'Buzz back',
    version: '1.1.0',
    description: 'Buzzes back.',
    watch: ['buzz-back'],
    file: 'buzz-back.ntplugin',
    bytes: plugin.length,
    sha256: sum,
  };
  const index = (format = 1) =>
    utf8Encode(JSON.stringify({ format, apps: [], plugins: [buzzBack, { id: 'gone', name: 'Gone', file: 'gone.ntplugin' }] }));
  const INDEX_URL = 'https://api/assets/21';
  const PLUGIN_URL = 'https://api/assets/22';
  const storeReleases = [
    {
      id: 1,
      tag_name: 'v1',
      name: 'Version 1',
      published_at: '2026-10-15T00:00:00Z',
      prerelease: false,
      assets: [{ id: 11, name: 'store.json', size: 1, url: 'https://api/assets/11', updated_at: '2026-10-15T00:00:00Z' }],
    },
    {
      id: 2,
      tag_name: 'apps-latest',
      name: 'Apps (latest master)',
      published_at: '2026-10-01T00:00:00Z',
      prerelease: true,
      assets: [
        { id: 21, name: 'store.json', size: 1, url: INDEX_URL, updated_at: '2026-10-16T00:00:00Z' },
        { id: 22, name: 'buzz-back.ntplugin', size: plugin.length, url: PLUGIN_URL },
      ],
    },
    { id: 3, tag_name: 'v0', name: null, published_at: '2026-10-17T00:00:00Z', prerelease: false, assets: [] },
  ];
  const listed = () => listOnlinePlugins('a/b', { fetcher: fakeFetch({ [RELEASES]: storeReleases, [INDEX_URL]: index() }) });

  it('lists the plugins of the release whose store.json changed last, leaving out any without a file', async () => {
    const calls: Call[] = [];
    const found = await listOnlinePlugins('a/b', {
      fetcher: fakeFetch({ [RELEASES]: storeReleases, [INDEX_URL]: index() }, calls),
    });
    expect(found.map((item) => item.entry)).toEqual([
      expect.objectContaining({
        key: 'plugin:buzz-back',
        version: '1.1.0',
        description: 'Buzzes back.',
        watch: ['buzz-back'],
        source: { type: 'github', repo: 'a/b', release: 'Apps (latest master)' },
      }),
    ]);
    expect(calls.map((call) => call.url)).toEqual([RELEASES, INDEX_URL]);
    expect(calls[1].headers.Accept).toBe('application/octet-stream');
  });

  it('finds nothing without a store.json, and refuses another format', async () => {
    expect(await listOnlinePlugins('a/b', { fetcher: fakeFetch({ [RELEASES]: [storeReleases[2]] }) })).toEqual([]);
    await expect(
      listOnlinePlugins('a/b', { fetcher: fakeFetch({ [RELEASES]: storeReleases, [INDEX_URL]: index(2) }) }),
    ).rejects.toThrow('another version');
  });

  it('downloads one plugin and checks its checksum and id', async () => {
    const [online] = await listed();
    const fetcher = fakeFetch({ [PLUGIN_URL]: plugin });
    expect((await fetchOnlinePlugin(online, { fetcher })).entry.key).toBe('plugin:buzz-back');
    await expect(fetchOnlinePlugin({ ...online, sha256: '00' }, { fetcher })).rejects.toThrow('checksum');
    const other = { ...online, sha256: '', entry: { ...online.entry, key: 'plugin:other' } };
    await expect(fetchOnlinePlugin(other, { fetcher })).rejects.toThrow('another plugin');
  });

  it('stands in for a kept plugin only when that came from GitHub at another version', async () => {
    const [online] = await listed();
    const kept = (version: string, source: StoreEntry['source']): StoreEntry => ({ ...online.entry, version, source });
    const fromGitHub = { type: 'github' as const, repo: 'a/b', release: 'v1' };
    const dice: StoreEntry = { ...online.entry, key: 'watch:dice', kind: 'app', id: 'dice', name: 'Dice', source: { type: 'bundled' } };

    const fresh = withOnlinePlugins([dice], [online]);
    expect(fresh.entries.map((entry) => entry.key)).toEqual(['watch:dice', 'plugin:buzz-back']);
    expect([...fresh.online.keys()]).toEqual(['plugin:buzz-back']);
    expect(withOnlinePlugins([kept('1.0.0', fromGitHub)], [online]).online.size).toBe(1);
    expect(withOnlinePlugins([kept('1.1.0', fromGitHub)], [online]).online.size).toBe(0);
    expect(withOnlinePlugins([kept('0.9.0', { type: 'file', name: 'mine.ntplugin' })], [online]).online.size).toBe(0);
  });
});
