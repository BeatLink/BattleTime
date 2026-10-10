import { describe, expect, it } from '@jest/globals';
import { strToU8, zipSync } from 'fflate';

import { downloadFirmware, listFirmwareBuilds } from './github-firmware';
import { answerHttpRequest, isHostAllowed, MAX_RESPONSE, parseHostList } from './http-proxy';
import { currentWeather, describeWeatherCode, findPlace } from './weather';

// A fetch that answers from a table of URL prefixes.
function fakeFetch(routes: Record<string, unknown>): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    const match = Object.keys(routes).find((prefix) => url.startsWith(prefix));
    if (!match) {
      return new Response('not found', { status: 404 });
    }
    const body = routes[match];
    return body instanceof Uint8Array
      ? new Response(body.slice().buffer)
      : new Response(typeof body === 'string' ? body : JSON.stringify(body));
  }) as typeof fetch;
}

describe('http proxy', () => {
  it('allows listed hosts and their subdomains only', () => {
    const allowed = parseHostList('example.com,  api.other.org\nthird.net');
    expect(allowed).toEqual(['example.com', 'api.other.org', 'third.net']);
    expect(isHostAllowed('example.com', allowed)).toBe(true);
    expect(isHostAllowed('www.example.com', allowed)).toBe(true);
    expect(isHostAllowed('badexample.com', allowed)).toBe(false);
    expect(isHostAllowed('other.org', allowed)).toBe(false);
  });

  it('answers with the page, cut short, or with an error', async () => {
    const fetcher = fakeFetch({ 'https://example.com/': 'x'.repeat(5000) });
    const ok = await answerHttpRequest({ t: 'http', id: '1', url: 'https://example.com/a' }, ['example.com'], fetcher);
    expect(ok.resp).toHaveLength(MAX_RESPONSE);
    const refused = await answerHttpRequest({ t: 'http', id: '2', url: 'https://evil.com/' }, ['example.com'], fetcher);
    expect(refused).toEqual({ t: 'http', id: '2', err: 'evil.com is not allowed' });
    const file = await answerHttpRequest({ t: 'http', id: '3', url: 'file:///etc/passwd' }, ['example.com'], fetcher);
    expect(file.err).toBe('only http and https');
  });
});

describe('weather', () => {
  it('turns Open-Meteo conditions into the watch message', async () => {
    const fetcher = fakeFetch({
      'https://geocoding-api.open-meteo.com/': { results: [{ name: 'Kingston', latitude: 18, longitude: -76.8 }] },
      'https://api.open-meteo.com/': {
        current: { temperature_2m: 30, relative_humidity_2m: 70.4, weather_code: 2, wind_speed_10m: 12.6 },
      },
    });
    const place = await findPlace('Kingston', fetcher);
    expect(await currentWeather(place, fetcher)).toEqual({
      t: 'weather',
      temp: 303.2,
      hum: 70,
      txt: 'Partly cloudy',
      wind: 13,
      loc: 'Kingston',
    });
    expect(describeWeatherCode(95)).toBe('Thunderstorm');
  });
});

describe('GitHub firmware', () => {
  const firmwareZip = zipSync({
    'manifest.json': strToU8(JSON.stringify({ manifest: { application: { bin_file: 'a.bin', dat_file: 'a.dat' } } })),
    'a.bin': new Uint8Array([1, 2, 3]),
    'a.dat': new Uint8Array([4]),
  });
  const artifactZip = zipSync({ 'micropython.zip': firmwareZip, 'bootloader.hex': strToU8(':00') });

  const fetcher = fakeFetch({
    'https://api.github.com/repos/wasp-os/wasp-os/releases': [
      {
        id: 1,
        tag_name: 'v0.5',
        name: null,
        published_at: '2026-01-01T00:00:00Z',
        prerelease: false,
        assets: [
          { id: 7, name: 'pinetime-micropython.zip', size: 300, browser_download_url: 'https://dl/release.zip' },
          { id: 8, name: 'p8-micropython.zip', size: 300, browser_download_url: 'https://dl/p8.zip' },
        ],
      },
    ],
    'https://api.github.com/repos/wasp-os/wasp-os/actions/artifacts': {
      artifacts: [
        {
          id: 9,
          name: 'pinetime-9128cdb675e5',
          size_in_bytes: 600,
          expired: false,
          created_at: '2026-02-01T00:00:00Z',
          archive_download_url: 'https://api.github.com/artifact/9/zip',
          workflow_run: { head_branch: 'master', head_sha: '9128cdb675e5' },
        },
        { id: 10, name: 'pinetime-old', size_in_bytes: 1, expired: true, created_at: '', archive_download_url: '' },
      ],
    },
    'https://dl/release.zip': firmwareZip,
    'https://api.github.com/artifact/9/zip': artifactZip,
  });

  it('lists builds for the board, newest first', async () => {
    const builds = await listFirmwareBuilds('wasp-os/wasp-os', 'pinetime', { fetcher });
    expect(builds.map((build) => build.title)).toEqual(['master at 9128cdb', 'v0.5']);
    expect(builds[0].needsToken).toBe(true);
  });

  it('unpacks the firmware from a release and from an artifact', async () => {
    const [artifact, release] = await listFirmwareBuilds('wasp-os/wasp-os', 'pinetime', { fetcher });
    expect((await downloadFirmware(release, { fetcher })).images[0].image).toEqual(new Uint8Array([1, 2, 3]));
    await expect(downloadFirmware(artifact, { fetcher })).rejects.toThrow('token');
    const fromArtifact = await downloadFirmware(artifact, { fetcher, token: 't' });
    expect(fromArtifact.images[0].part).toBe('application');
  });

  it('rejects a repository that is not owner/name', async () => {
    await expect(listFirmwareBuilds('not a repo', 'pinetime', { fetcher })).rejects.toThrow('owner/name');
  });
});
