import { describe, expect, it } from '@jest/globals';
import { zipSync } from 'fflate';

import { bytesToBase64, utf8Encode } from '@/ble/encoding';
import { BUNDLED_IMAGES } from '@/bundles/watch-apps';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { PackagesClient, type FileWriter } from '@/protocol/packages';
import { testImage } from '@/test-support/app-images';

import {
  AppStore,
  AppStoreError,
  bundledOffers,
  entryFromImage,
  installEntry,
  installState,
  offerFromFile,
  sourceText,
  uninstallEntry,
  type KeyValueStorage,
  type StoreEntry,
} from './app-store';
import { Plugins, type PluginBridge, type PluginStatus } from './plugins';

const bundle = [
  { file: 'timer.ntapp', description: 'Counts down.', base64: bytesToBase64(testImage('timer', 'Timer')) },
  { file: 'digital.ntapp', base64: bytesToBase64(testImage('digital', 'Digital', 'face')) },
  { file: 'shared.ntlib', base64: bytesToBase64(testImage('shared', 'Shared library', 'library')) },
  { file: 'prompt.ntapp', base64: bytesToBase64(testImage('prompt', 'Prompt')) },
];

function memoryStorage(): KeyValueStorage & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    getItem: async (key) => values.get(key) ?? null,
    setItem: async (key, value) => {
      values.set(key, value);
    },
    removeItem: async (key) => {
      values.delete(key);
    },
  };
}

function pluginFile(id: string, watchId: string | null): Uint8Array {
  const files: Record<string, Uint8Array> = {};
  const manifest = {
    id,
    name: 'Buzz back',
    version: '1.2.0',
    description: 'Buzzes back.',
    watch: watchId ? [`watch/${watchId}.ntapp`] : [],
    permissions: { messages: [], hosts: [] },
  };
  files['manifest.json'] = utf8Encode(JSON.stringify(manifest));
  if (watchId) files[`watch/${watchId}.ntapp`] = testImage(watchId, 'Buzz');
  return zipSync(files);
}

describe('store entries', () => {
  it('reads an app, a face and a plugin by their contents', () => {
    const app = offerFromFile(testImage('dice', 'Dice', 'app', 0, 'games'), { type: 'file', name: 'x.bin' }, 'Rolls.');
    expect(app.entry).toMatchObject({ key: 'watch:dice', kind: 'app', name: 'Dice', group: 'games', description: 'Rolls.' });

    const plugin = offerFromFile(pluginFile('buzz-back', 'buzz'), { type: 'file', name: 'b.ntplugin' });
    expect(plugin.entry).toMatchObject({
      key: 'plugin:buzz-back',
      kind: 'plugin',
      version: '1.2.0',
      description: 'Buzzes back.',
      watch: ['buzz'],
    });
  });

  it('refuses the shared library and files that are not apps or plugins', () => {
    expect(() => entryFromImage(testImage('shared', 'Shared', 'library'), { type: 'bundled' })).toThrow(AppStoreError);
    expect(() => offerFromFile(utf8Encode('PK not a zip'), { type: 'bundled' })).toThrow('not a NeoTime plugin');
    expect(() => offerFromFile(zipSync({ 'a.txt': utf8Encode('a') }), { type: 'bundled' })).toThrow('no manifest');
    expect(() => offerFromFile(zipSync({ 'manifest.json': utf8Encode('{}') }), { type: 'bundled' })).toThrow('id and name');
  });

  it('starts from the bundle without the library or the temporary apps', () => {
    expect(bundledOffers(bundle).map((offer) => [offer.entry.id, offer.entry.description])).toEqual([
      ['timer', 'Counts down.'],
      ['digital', ''],
    ]);
  });

  it('offers every app and face the companion bundles', () => {
    const kinds = new Set(bundledOffers(BUNDLED_IMAGES).map((offer) => offer.entry.kind));
    expect(kinds).toEqual(new Set(['app', 'face']));
    expect(bundledOffers(BUNDLED_IMAGES).length).toBeGreaterThan(40);
  });

  it('names where an entry came from', () => {
    expect(sourceText({ type: 'bundled' })).toBe('Bundled');
    expect(sourceText({ type: 'github', repo: 'a/b', release: 'v1' })).toBe('a/b v1');
    expect(sourceText({ type: 'file', name: 'x.ntapp' })).toBe('x.ntapp');
  });
});

describe('the store', () => {
  it('adds, replaces, removes and restores entries, keeping each file', async () => {
    const storage = memoryStorage();
    const store = new AppStore(storage, bundle);
    expect((await store.entries()).map((entry) => entry.key)).toEqual(['watch:timer', 'watch:digital']);

    const newer = testImage('timer', 'Timer 2', 'app', 10);
    await store.add([
      offerFromFile(newer, { type: 'github', repo: 'a/b', release: 'v2' }),
      offerFromFile(pluginFile('buzz-back', 'buzz'), { type: 'file', name: 'b.ntplugin' }),
    ]);
    const entries = await store.entries();
    expect(entries.map((entry) => entry.name)).toEqual(['Timer 2', 'Digital', 'Buzz back']);
    expect(await store.data('watch:timer')).toEqual(newer);

    await store.remove('watch:timer');
    await store.remove('watch:digital');
    expect((await store.entries()).map((entry) => entry.key)).toEqual(['plugin:buzz-back']);
    expect(await store.hiddenCount()).toBe(2);
    expect(storage.values.has('neotime.store.data.watch:timer')).toBe(false);

    await store.restoreBundled();
    expect((await store.entries()).map((entry) => entry.name)).toEqual(['Timer', 'Digital', 'Buzz back']);
    expect(await store.data('watch:timer')).toEqual(testImage('timer', 'Timer'));
    await expect(store.data('watch:nothing')).rejects.toThrow('no file');
  });

  it('brings back a removed bundled entry when the same app is added again', async () => {
    const store = new AppStore(memoryStorage(), bundle);
    await store.remove('watch:digital');
    await store.add([offerFromFile(testImage('digital', 'Digital'), { type: 'file', name: 'd.ntapp' })]);
    expect(await store.hiddenCount()).toBe(0);
  });

  it('starts empty when what was saved cannot be read', async () => {
    const storage = memoryStorage();
    storage.values.set('neotime.store.v1', '{not json');
    expect((await new AppStore(storage, bundle).entries()).length).toBe(2);
  });
});

// ============================================================================
// Installing
// ============================================================================

const entry = (overrides: Partial<StoreEntry>): StoreEntry => ({
  key: 'watch:timer',
  kind: 'app',
  id: 'timer',
  name: 'Timer',
  description: '',
  group: '',
  version: '',
  watch: [],
  bytes: 1,
  source: { type: 'bundled' },
  ...overrides,
});

function fakePlugins(): { plugins: Plugins; installed: number[][]; removed: string[] } {
  const installed: number[][] = [];
  const removed: string[] = [];
  const bridge: PluginBridge = {
    list: async () => ({ ok: true, value: [] }),
    installPick: async () => ({ ok: true, value: null }),
    install: async (data) => {
      installed.push(data);
      return { ok: true, value: undefined as unknown as PluginStatus };
    },
    remove: async (id) => {
      removed.push(id);
      return { ok: true };
    },
    setEnabled: async () => ({ ok: true }),
    watchApps: async () => ({ ok: true, value: [{ name: 'buzz.ntapp', data: Array.from(testImage('buzz', 'Buzz')) }] }),
    uiMessage: async () => ({ ok: true }),
    onUi: () => () => {},
    onLog: () => () => {},
  };
  return { plugins: new Plugins(bridge), installed, removed };
}

async function watchTargets() {
  const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
  const files: FileWriter = {
    write: async (path, data) => {
      target.fileService.files.set(path, data);
    },
  };
  return { target, files, packages: await PackagesClient.open(target) };
}

describe('installing from the store', () => {
  it('says whether each entry is installed', () => {
    const apps = [{ id: 'timer', name: 'Timer', kind: 'app' as const, enabled: true, bytes: 1, need: null, group: '' }];
    const plugin = entry({ kind: 'plugin', id: 'buzz-back' });
    const status = { manifest: { id: 'buzz-back' }, enabled: true, running: true } as PluginStatus;
    expect(installState(entry({}), apps, null)).toBe('installed');
    expect(installState(entry({ id: 'dice' }), apps, null)).toBe('not-installed');
    expect(installState(plugin, apps, null)).toBe('plugin-elsewhere');
    expect(installState(plugin, apps, [status])).toBe('installed');
    expect(installState(plugin, apps, [])).toBe('not-installed');
  });

  it('installs and uninstalls an app on the watch', async () => {
    const { files, packages } = await watchTargets();
    await installEntry(entry({}), testImage('timer', 'Timer'), { files, packages, plugins: null });
    expect((await packages.apps()).map((app) => app.id)).toEqual(['timer']);
    await uninstallEntry(entry({}), { files, packages, plugins: null });
    expect(await packages.apps()).toEqual([]);
  });

  it('installs a plugin in the desktop app with its watch app, and uninstalls both', async () => {
    const { files, packages } = await watchTargets();
    const { plugins, installed, removed } = fakePlugins();
    const plugin = entry({ key: 'plugin:buzz-back', kind: 'plugin', id: 'buzz-back', watch: ['buzz', 'gone'] });

    await installEntry(plugin, Uint8Array.of(1, 2), { files, packages, plugins });
    expect(installed).toEqual([[1, 2]]);
    expect((await packages.apps()).map((app) => app.id)).toEqual(['buzz']);

    await uninstallEntry(plugin, { files, packages, plugins });
    expect(await packages.apps()).toEqual([]);
    expect(removed).toEqual(['buzz-back']);
  });

  it('refuses plugins where they cannot run', async () => {
    const { files, packages } = await watchTargets();
    const plugin = entry({ kind: 'plugin', id: 'buzz-back' });
    await expect(installEntry(plugin, Uint8Array.of(1), { files, packages, plugins: null })).rejects.toThrow('desktop app');
    await expect(uninstallEntry(plugin, { files, packages, plugins: null })).rejects.toThrow('desktop app');
  });
});
