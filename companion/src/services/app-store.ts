// The app store: the apps, faces and plugins the companion keeps ready to install on the watch.
//
// It starts with every image the companion bundles, and the user adds more from files or GitHub
// releases and removes any they do not want. Each added file is kept here, so it installs without a
// download. docs/app-store.md describes the store and the files a release carries for it.

import { unzipSync } from 'fflate';

import { base64ToBytes, bytesToBase64, utf8Decode } from '@/ble/encoding';
import { BUNDLED_IMAGES, type BundledImage } from '@/bundles/watch-apps';
import { readImageHeader } from '@/protocol/app-image';
import { installImage, type FileWriter, type PackagesClient, type Progress, type WatchApp } from '@/protocol/packages';
import { TEMPORARY_ONLY_IDS } from '@/protocol/prompt';

import { installPluginWatchApps, type PluginManifest, type Plugins, type PluginStatus } from './plugins';

// ============================================================================
// Entries
// ============================================================================

export type StoreKind = 'app' | 'face' | 'plugin';

// Where an entry came from: the companion's own bundle, a GitHub release or build, or a file the user chose.
export type StoreSource =
  | { type: 'bundled' }
  | { type: 'github'; repo: string; release: string }
  | { type: 'file'; name: string };

// `key` is unique in the store: apps and faces share the watch's ids, and plugins have their own.
// `watch` lists the ids of the watch apps a plugin carries; it is empty for an app or face.
export type StoreEntry = {
  key: string;
  kind: StoreKind;
  id: string;
  name: string;
  description: string;
  group: string;
  version: string;
  watch: string[];
  bytes: number;
  source: StoreSource;
};

// An entry with its file, ready to be added to the store.
export type StoreOffer = { entry: StoreEntry; data: Uint8Array };

export class AppStoreError extends Error {}

export function storeKey(kind: StoreKind, id: string): string {
  return kind === 'plugin' ? `plugin:${id}` : `watch:${id}`;
}

// The image's own header names it; the description comes from wherever it was found.
export function entryFromImage(image: Uint8Array, source: StoreSource, description = ''): StoreEntry {
  const header = readImageHeader(image);
  if (header.kind === 'library') {
    throw new AppStoreError('The shared library ships inside the firmware, so the store does not keep it.');
  }
  return {
    key: storeKey(header.kind, header.id),
    kind: header.kind,
    id: header.id,
    name: header.name,
    description,
    group: header.group,
    version: '',
    watch: [],
    bytes: image.length,
    source,
  };
}

// A .ntplugin is a zip whose manifest.json names it and lists the watch apps inside.
export function entryFromPlugin(data: Uint8Array, source: StoreSource): StoreEntry {
  let files: Record<string, Uint8Array>;
  try {
    files = unzipSync(data);
  } catch {
    throw new AppStoreError('That file is not a NeoTime plugin.');
  }
  const manifestFile = files['manifest.json'];
  if (!manifestFile) {
    throw new AppStoreError('That plugin has no manifest.json.');
  }
  const manifest = JSON.parse(utf8Decode(manifestFile)) as Partial<PluginManifest>;
  if (typeof manifest.id !== 'string' || typeof manifest.name !== 'string') {
    throw new AppStoreError('That plugin does not say its id and name.');
  }
  const watch = (manifest.watch ?? []).flatMap((path) => (files[path] ? [readImageHeader(files[path]).id] : []));
  return {
    key: storeKey('plugin', manifest.id),
    kind: 'plugin',
    id: manifest.id,
    name: manifest.name,
    description: manifest.description ?? '',
    group: '',
    version: manifest.version ?? '',
    watch,
    bytes: data.length,
    source,
  };
}

// Reads an app, face or plugin file by its contents, whatever it is called.
export function offerFromFile(data: Uint8Array, source: StoreSource, description = ''): StoreOffer {
  const isZip = data[0] === 0x50 && data[1] === 0x4b;
  const entry = isZip ? entryFromPlugin(data, source) : entryFromImage(data, source, description);
  return { entry, data };
}

// The bundle's apps and faces, without the library and the apps the companion only runs for a moment.
export function bundledOffers(bundle: BundledImage[] = BUNDLED_IMAGES): StoreOffer[] {
  return bundle.flatMap((item) => {
    const data = base64ToBytes(item.base64);
    const header = readImageHeader(data);
    if (header.kind === 'library' || TEMPORARY_ONLY_IDS.has(header.id)) {
      return [];
    }
    return [{ entry: entryFromImage(data, { type: 'bundled' }, item.description ?? ''), data }];
  });
}

const KIND_ORDER: StoreKind[] = ['app', 'face', 'plugin'];

export function sortEntries(entries: StoreEntry[]): StoreEntry[] {
  return [...entries].sort(
    (a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind) || a.name.localeCompare(b.name),
  );
}

export function sourceText(source: StoreSource): string {
  switch (source.type) {
    case 'bundled':
      return 'Bundled';
    case 'github':
      return `${source.repo} ${source.release}`;
    case 'file':
      return source.name;
  }
}

// ============================================================================
// Keeping the store
// ============================================================================

// What the store keeps its list and files in: AsyncStorage in the app, a plain map in tests.
export type KeyValueStorage = {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
  removeItem(key: string): Promise<void>;
};

const LIST_KEY = 'neotime.store.v1';
const DATA_KEY_PREFIX = 'neotime.store.data.';

// The entries the user added, and the keys of bundled entries the user removed.
type StoredList = { added: StoreEntry[]; hidden: string[] };

export class AppStore {
  private readonly bundled: Map<string, StoreOffer>;

  constructor(
    private readonly storage: KeyValueStorage,
    bundle: BundledImage[] = BUNDLED_IMAGES,
  ) {
    this.bundled = new Map(bundledOffers(bundle).map((offer) => [offer.entry.key, offer]));
  }

  private async stored(): Promise<StoredList> {
    try {
      const raw = await this.storage.getItem(LIST_KEY);
      const parsed = raw ? (JSON.parse(raw) as Partial<StoredList>) : {};
      return { added: parsed.added ?? [], hidden: parsed.hidden ?? [] };
    } catch {
      return { added: [], hidden: [] };
    }
  }

  private async save(list: StoredList): Promise<void> {
    await this.storage.setItem(LIST_KEY, JSON.stringify(list));
  }

  // Every entry, an added one in place of the bundled one with the same key.
  async entries(): Promise<StoreEntry[]> {
    const { added, hidden } = await this.stored();
    const byKey = new Map<string, StoreEntry>();
    for (const [key, offer] of this.bundled) {
      if (!hidden.includes(key)) byKey.set(key, offer.entry);
    }
    for (const entry of added) byKey.set(entry.key, entry);
    return sortEntries([...byKey.values()]);
  }

  // How many bundled entries the user removed, which restoreBundled brings back.
  async hiddenCount(): Promise<number> {
    return (await this.stored()).hidden.length;
  }

  // Adds each offer, replacing any entry with the same key.
  async add(offers: StoreOffer[]): Promise<void> {
    const list = await this.stored();
    for (const { entry, data } of offers) {
      await this.storage.setItem(DATA_KEY_PREFIX + entry.key, bytesToBase64(data));
      list.added = [...list.added.filter((each) => each.key !== entry.key), entry];
      list.hidden = list.hidden.filter((key) => key !== entry.key);
    }
    await this.save(list);
  }

  // Takes the entry out of the store; a bundled one stays out until restoreBundled.
  async remove(key: string): Promise<void> {
    const list = await this.stored();
    list.added = list.added.filter((entry) => entry.key !== key);
    if (this.bundled.has(key) && !list.hidden.includes(key)) {
      list.hidden.push(key);
    }
    await this.storage.removeItem(DATA_KEY_PREFIX + key);
    await this.save(list);
  }

  async restoreBundled(): Promise<void> {
    const list = await this.stored();
    await this.save({ ...list, hidden: [] });
  }

  // The entry's file: the added copy when there is one, otherwise the bundled image.
  async data(key: string): Promise<Uint8Array> {
    const { added } = await this.stored();
    if (added.some((entry) => entry.key === key)) {
      const base64 = await this.storage.getItem(DATA_KEY_PREFIX + key);
      if (base64) return base64ToBytes(base64);
    }
    const bundled = this.bundled.get(key);
    if (!bundled) {
      throw new AppStoreError(`The store has no file for ${key}.`);
    }
    return bundled.data;
  }
}

// ============================================================================
// Installing on the watch
// ============================================================================

// What installing and uninstalling reach: the watch's files and packages, and the desktop app's
// plugins, which are null where plugins cannot run.
export type StoreTargets = {
  files: FileWriter;
  packages: PackagesClient;
  plugins: Plugins | null;
};

export type InstallState = 'installed' | 'not-installed' | 'plugin-elsewhere';

// An app or face is installed when the watch lists it; a plugin when the desktop app holds it.
export function installState(entry: StoreEntry, watchApps: WatchApp[], plugins: PluginStatus[] | null): InstallState {
  if (entry.kind !== 'plugin') {
    return watchApps.some((app) => app.id === entry.id) ? 'installed' : 'not-installed';
  }
  if (plugins === null) {
    return 'plugin-elsewhere';
  }
  return plugins.some((status) => status.manifest.id === entry.id) ? 'installed' : 'not-installed';
}

function needPlugins(targets: StoreTargets): Plugins {
  if (!targets.plugins) {
    throw new AppStoreError('Plugins run only in the desktop app.');
  }
  return targets.plugins;
}

// Sends an app or face to the watch, or installs a plugin in the desktop app with its watch apps.
export async function installEntry(
  entry: StoreEntry,
  data: Uint8Array,
  targets: StoreTargets,
  onProgress?: (progress: Progress) => void,
): Promise<void> {
  if (entry.kind !== 'plugin') {
    await installImage(targets.files, targets.packages, data, onProgress);
    return;
  }
  const plugins = needPlugins(targets);
  await plugins.install(data);
  await installPluginWatchApps(plugins, entry.id, (image) =>
    installImage(targets.files, targets.packages, image, onProgress),
  );
}

// Removes an app or face from the watch, or a plugin from the desktop app with its watch apps.
export async function uninstallEntry(entry: StoreEntry, targets: StoreTargets): Promise<void> {
  if (entry.kind !== 'plugin') {
    await targets.packages.remove(entry.id);
    return;
  }
  const plugins = needPlugins(targets);
  const onWatch = await targets.packages.apps();
  for (const id of entry.watch) {
    if (onWatch.some((app) => app.id === id)) {
      await targets.packages.remove(id);
    }
  }
  await plugins.remove(entry.id);
}
