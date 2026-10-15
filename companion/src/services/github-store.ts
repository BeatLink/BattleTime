// Finds apps, faces and plugins on GitHub for the store: on releases, as battletime-store.zip or loose
// .ntapp and .ntplugin files, and in the battletime-store artifact each Actions run uploads. An artifact
// can only be downloaded with a GitHub token, even from a public repository.
//
// GitHub's API answers a window's fetch, but the host it redirects downloads to does not, so files
// come down through the desktop or Android app's native requests where there are some.

import { unzipSync } from 'fflate';

import { utf8Decode } from '@/ble/encoding';

import { AppStoreError, offerFromFile, type StoreOffer, type StoreSource } from './app-store';
import { checkRepo } from './github-firmware';
import { fetchRequester, webRequester, type WebRequester } from './web-request';

type Fetch = typeof fetch;

// `fetcher` lists and, unless `requester` is given, also downloads; `requester` downloads the files.
type Options = { token?: string; fetcher?: Fetch; requester?: WebRequester };

// How long one file may take to come down.
const DOWNLOAD_TIMEOUT_MS = 60000;

const API = 'https://api.github.com';

// tools/build_store.py's names: the zip holding everything, its list, and the artifact CI uploads.
export const STORE_ZIP = 'battletime-store.zip';
export const STORE_INDEX = 'store.json';
export const STORE_ARTIFACT = 'battletime-store';

// The store.json format this app reads.
const STORE_FORMAT = 1;

export type StoreFile = { name: string; url: string; sizeBytes: number };

// A release or Actions build with something for the store. `files` are its loose app and plugin
// files, and `bundle` its battletime-store.zip, which holds them all in one download.
export type StoreRelease = {
  id: string;
  source: 'release' | 'actions';
  title: string;
  date: string;
  bundle: StoreFile | null;
  files: StoreFile[];
  needsToken: boolean;
};

type ReleaseJson = {
  id: number;
  tag_name: string;
  name: string | null;
  published_at: string;
  prerelease: boolean;
  assets: { id: number; name: string; size: number; url: string }[];
};

type ArtifactJson = {
  id: number;
  name: string;
  size_in_bytes: number;
  expired: boolean;
  created_at: string;
  archive_download_url: string;
  workflow_run?: { head_branch?: string; head_sha?: string };
};

function isStoreFile(name: string): boolean {
  return name.endsWith('.ntapp') || name.endsWith('.ntplugin');
}

function headers(token: string | undefined, accept = 'application/vnd.github+json'): Record<string, string> {
  return token ? { Accept: accept, Authorization: `Bearer ${token}` } : { Accept: accept };
}

async function getJson<T>(url: string, token: string | undefined, fetcher: Fetch): Promise<T> {
  const response = await fetcher(url, { headers: headers(token) });
  if (!response.ok) {
    throw new Error(`GitHub answered ${response.status} for ${url.replace(API, '')}.`);
  }
  return (await response.json()) as T;
}

// ============================================================================
// Listing
// ============================================================================

// Every release and unexpired Actions build that carries apps or plugins, newest first. Actions builds
// are listed only with a token, since without one they could not be downloaded.
export async function listStoreReleases(repo: string, options: Options = {}): Promise<StoreRelease[]> {
  const fetcher = options.fetcher ?? fetch;
  const name = checkRepo(repo);
  const found: StoreRelease[] = [];

  const releases = await getJson<ReleaseJson[]>(`${API}/repos/${name}/releases?per_page=20`, options.token, fetcher);
  for (const release of releases) {
    // The API's asset URL, unlike the browser download link, answers a web page's request.
    const toFile = (asset: ReleaseJson['assets'][number]): StoreFile => ({
      name: asset.name,
      url: asset.url,
      sizeBytes: asset.size,
    });
    const bundle = release.assets.find((asset) => asset.name === STORE_ZIP);
    const files = release.assets.filter((asset) => isStoreFile(asset.name)).map(toFile);
    if (!bundle && files.length === 0) {
      continue;
    }
    found.push({
      id: `release-${release.id}`,
      source: 'release',
      title: `${release.name || release.tag_name}${release.prerelease ? ' (pre-release)' : ''}`,
      date: release.published_at,
      bundle: bundle ? toFile(bundle) : null,
      files,
      needsToken: false,
    });
  }

  if (options.token) {
    const artifacts = await getJson<{ artifacts: ArtifactJson[] }>(
      `${API}/repos/${name}/actions/artifacts?per_page=50&name=${STORE_ARTIFACT}`,
      options.token,
      fetcher,
    );
    for (const artifact of artifacts.artifacts) {
      if (artifact.expired || artifact.name !== STORE_ARTIFACT) {
        continue;
      }
      const branch = artifact.workflow_run?.head_branch;
      const sha = (artifact.workflow_run?.head_sha ?? '').slice(0, 7);
      found.push({
        id: `actions-${artifact.id}`,
        source: 'actions',
        title: `${branch ? `${branch} at ` : 'Build '}${sha || artifact.id}`,
        date: artifact.created_at,
        bundle: { name: `${STORE_ARTIFACT}.zip`, url: artifact.archive_download_url, sizeBytes: artifact.size_in_bytes },
        files: [],
        needsToken: true,
      });
    }
  }

  return found.sort((a, b) => b.date.localeCompare(a.date));
}

// ============================================================================
// Reading what was downloaded
// ============================================================================

type StoreIndex = {
  format: number;
  apps?: { file: string; description?: string }[];
  plugins?: { file: string }[];
};

// A store zip, or an artifact holding the same files: store.json lists what is inside and describes
// each app; a zip without one offers every app and plugin file it holds.
export function offersFromStoreZip(data: Uint8Array, source: StoreSource): StoreOffer[] {
  let files: Record<string, Uint8Array>;
  try {
    files = unzipSync(data);
  } catch {
    throw new AppStoreError('The download is not a zip.');
  }
  const indexFile = files[STORE_INDEX];
  if (!indexFile) {
    return Object.keys(files)
      .filter(isStoreFile)
      .sort()
      .map((name) => offerFromFile(files[name], source));
  }
  const index = JSON.parse(utf8Decode(indexFile)) as StoreIndex;
  if (index.format !== STORE_FORMAT) {
    throw new AppStoreError('That store was made for another version of the companion.');
  }
  const listed = [
    ...(index.apps ?? []).map((app) => ({ file: app.file, description: app.description ?? '' })),
    ...(index.plugins ?? []).map((plugin) => ({ file: plugin.file, description: '' })),
  ];
  return listed.flatMap(({ file, description }) =>
    files[file] ? [offerFromFile(files[file], source, description)] : [],
  );
}

// ============================================================================
// Downloading
// ============================================================================

async function download(file: StoreFile, token: string | undefined, requester: WebRequester): Promise<Uint8Array> {
  const reply = await requester({
    method: 'GET',
    url: file.url,
    headers: headers(token, 'application/octet-stream'),
    access: {},
    timeoutMs: DOWNLOAD_TIMEOUT_MS,
  });
  if (reply.status < 200 || reply.status >= 300) {
    throw new Error(`Downloading ${file.name} failed (${reply.status}).`);
  }
  return reply.body;
}

// Downloads what a release or build offers: its store zip in one go, or else each loose file.
export async function fetchStoreRelease(
  repo: string,
  release: StoreRelease,
  options: Options = {},
): Promise<StoreOffer[]> {
  if (release.needsToken && !options.token) {
    throw new Error('GitHub only hands out Actions builds with a token. Add one in Settings.');
  }
  const requester = options.requester ?? (options.fetcher ? fetchRequester(options.fetcher) : webRequester());
  const source: StoreSource = { type: 'github', repo: checkRepo(repo), release: release.title };
  if (release.bundle) {
    return offersFromStoreZip(await download(release.bundle, options.token, requester), source);
  }
  const offers: StoreOffer[] = [];
  for (const file of release.files) {
    offers.push(offerFromFile(await download(file, options.token, requester), source));
  }
  return offers;
}
