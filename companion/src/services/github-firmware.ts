// Finds wasp-os firmware builds on GitHub: zip assets on releases, and the
// artifacts each Actions run uploads. An artifact can only be downloaded with
// a GitHub token, even from a public repository.

import { unzipSync } from 'fflate';

import { readFirmwarePackage, type FirmwarePackage } from '@/dfu/package';

export type FirmwareBuild = {
  id: string;
  source: 'release' | 'actions';
  title: string;
  detail: string;
  date: string;
  sizeBytes: number;
  url: string;
  needsToken: boolean;
};

type Fetch = typeof fetch;

const API = 'https://api.github.com';

// The firmware a build carries for the watch; the rest of an artifact is bootloaders.
const FIRMWARE_FILE = 'micropython.zip';

function headers(token?: string): Record<string, string> {
  const base: Record<string, string> = { Accept: 'application/vnd.github+json' };
  return token ? { ...base, Authorization: `Bearer ${token}` } : base;
}

export function checkRepo(repo: string): string {
  const trimmed = repo.trim();
  if (!/^[\w.-]+\/[\w.-]+$/.test(trimmed)) {
    throw new Error('Give the repository as owner/name, for example wasp-os/wasp-os.');
  }
  return trimmed;
}

async function getJson<T>(url: string, token: string | undefined, fetcher: Fetch): Promise<T> {
  const response = await fetcher(url, { headers: headers(token) });
  if (!response.ok) {
    throw new Error(`GitHub answered ${response.status} for ${url.replace(API, '')}.`);
  }
  return (await response.json()) as T;
}

type ReleaseJson = {
  id: number;
  tag_name: string;
  name: string | null;
  published_at: string;
  prerelease: boolean;
  assets: { id: number; name: string; size: number; browser_download_url: string }[];
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

// List what can be flashed to a board, newest first.
export async function listFirmwareBuilds(
  repo: string,
  board: string,
  options: { token?: string; fetcher?: Fetch } = {},
): Promise<FirmwareBuild[]> {
  const fetcher = options.fetcher ?? fetch;
  const name = checkRepo(repo);
  const builds: FirmwareBuild[] = [];

  const releases = await getJson<ReleaseJson[]>(`${API}/repos/${name}/releases?per_page=10`, options.token, fetcher);
  for (const release of releases) {
    for (const asset of release.assets) {
      const lower = asset.name.toLowerCase();
      if (!lower.endsWith('.zip') || !(lower.includes(board) || lower === FIRMWARE_FILE)) {
        continue;
      }
      builds.push({
        id: `release-${asset.id}`,
        source: 'release',
        title: `${release.name || release.tag_name}${release.prerelease ? ' (pre-release)' : ''}`,
        detail: asset.name,
        date: release.published_at,
        sizeBytes: asset.size,
        url: asset.browser_download_url,
        needsToken: false,
      });
    }
  }

  const artifacts = await getJson<{ artifacts: ArtifactJson[] }>(
    `${API}/repos/${name}/actions/artifacts?per_page=50`,
    options.token,
    fetcher,
  );
  for (const artifact of artifacts.artifacts) {
    if (artifact.expired || !artifact.name.startsWith(`${board}-`)) {
      continue;
    }
    const branch = artifact.workflow_run?.head_branch;
    const sha = (artifact.workflow_run?.head_sha ?? artifact.name.slice(board.length + 1)).slice(0, 7);
    builds.push({
      id: `actions-${artifact.id}`,
      source: 'actions',
      title: branch ? `${branch} at ${sha}` : sha,
      detail: 'Actions build',
      date: artifact.created_at,
      sizeBytes: artifact.size_in_bytes,
      url: artifact.archive_download_url,
      needsToken: true,
    });
  }

  return builds.sort((a, b) => b.date.localeCompare(a.date));
}

// An artifact is a zip holding the firmware zip alongside the bootloaders; a release asset is the firmware zip itself.
export function firmwareFromDownload(data: Uint8Array, build: FirmwareBuild): FirmwarePackage {
  if (build.source === 'actions') {
    const files = unzipSync(data, { filter: (file) => file.name.endsWith(FIRMWARE_FILE) });
    const inner = Object.values(files)[0];
    if (!inner) {
      throw new Error(`The build has no ${FIRMWARE_FILE}.`);
    }
    return readFirmwarePackage(inner, `${build.title} ${FIRMWARE_FILE}`);
  }
  return readFirmwarePackage(data, build.detail);
}

export async function downloadFirmware(
  build: FirmwareBuild,
  options: { token?: string; fetcher?: Fetch } = {},
): Promise<FirmwarePackage> {
  if (build.needsToken && !options.token) {
    throw new Error('GitHub only hands out Actions builds with a token. Add one in Settings.');
  }
  const fetcher = options.fetcher ?? fetch;
  const response = await fetcher(build.url, { headers: build.needsToken ? headers(options.token) : {} });
  if (!response.ok) {
    throw new Error(`Downloading the build failed (${response.status}).`);
  }
  return firmwareFromDownload(new Uint8Array(await response.arrayBuffer()), build);
}
