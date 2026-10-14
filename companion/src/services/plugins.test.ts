import { afterEach, describe, expect, it } from '@jest/globals';

import {
  appendLog,
  describePermissions,
  installPluginWatchApps,
  messageFromPage,
  messageToPage,
  pluginBridge,
  pluginPageUrl,
  Plugins,
  pluginStateText,
  type PluginBridge,
  type PluginManifest,
  type PluginStatus,
} from './plugins';

const manifest: PluginManifest = {
  id: 'weather-demo',
  name: 'Weather demo',
  version: '1.0.0',
  description: '',
  watch: ['watch/weather-demo.ntapp'],
  main: 'phone/main.js',
  ui: 'phone/ui/index.html',
  permissions: { messages: ['weather', 'custom'], hosts: ['api.open-meteo.com'] },
};

const status: PluginStatus = { manifest, enabled: true, running: true };

function fakeBridge(overrides: Partial<PluginBridge> = {}): PluginBridge {
  return {
    list: async () => ({ ok: true, value: [status] }),
    installPick: async () => ({ ok: true, value: null }),
    install: async (data) => ({ ok: true, value: { ...status, manifest: { ...manifest, version: String(data.length) } } }),
    remove: async () => ({ ok: true }),
    setEnabled: async () => ({ ok: true }),
    watchApps: async () => ({ ok: true, value: [{ name: 'a.ntapp', data: [1, 2] }, { name: 'b.ntapp', data: [3] }] }),
    uiMessage: async () => ({ ok: true }),
    onUi: () => () => {},
    onLog: () => () => {},
    ...overrides,
  };
}

describe('plugins', () => {
  afterEach(() => {
    delete (globalThis as { neotime?: unknown }).neotime;
  });

  it('finds the bridge only in the desktop app', () => {
    expect(pluginBridge()).toBeNull();
    const bridge = fakeBridge();
    (globalThis as { neotime?: unknown }).neotime = { plugins: bridge };
    expect(pluginBridge()).toBe(bridge);
  });

  it('unwraps results and turns a refusal into an error', async () => {
    const calls: unknown[] = [];
    const plugins = new Plugins(
      fakeBridge({
        setEnabled: async (id, enabled) => {
          calls.push([id, enabled]);
          return { ok: true };
        },
        remove: async () => ({ ok: false, error: 'No plugin "x" is installed.' }),
        uiMessage: async () => ({ ok: false }),
      }),
    );
    expect(await plugins.list()).toEqual([status]);
    expect(await plugins.installPick()).toBeNull();
    expect((await plugins.install(Uint8Array.of(1, 2, 3))).manifest.version).toBe('3');
    await plugins.setEnabled('weather-demo', false);
    expect(calls).toEqual([['weather-demo', false]]);
    await expect(plugins.remove('x')).rejects.toThrow('No plugin "x" is installed.');
    await expect(plugins.uiMessage('x', {})).rejects.toThrow('could not reach its plugins');
  });

  it('installs each watch app a plugin carries, in order', async () => {
    const installed: number[][] = [];
    const count = await installPluginWatchApps(new Plugins(fakeBridge()), 'weather-demo', async (image) => {
      installed.push(Array.from(image));
    });
    expect(count).toBe(2);
    expect(installed).toEqual([[1, 2], [3]]);
  });

  it('puts permissions in words', () => {
    expect(describePermissions(manifest)).toEqual([
      'Send the weather to the watch',
      'Send custom messages to the watch',
      'Fetch from api.open-meteo.com',
    ]);
    const reading = { ...manifest, permissions: { messages: [], hosts: [], data: ['steps', 'other'] } };
    expect(describePermissions(reading)).toEqual(["Read the watch's step history", "Read the watch's other"]);
  });

  it('says why a plugin is or is not running', () => {
    expect(pluginStateText(status)).toBe('Running');
    expect(pluginStateText({ ...status, running: false })).toContain('Stopped by an error');
    expect(pluginStateText({ ...status, enabled: false })).toBe('Off');
    expect(pluginStateText({ ...status, manifest: { ...manifest, main: null } })).toContain('no logic');
  });

  it('wraps messages between the window and the settings page', () => {
    expect(pluginPageUrl('weather-demo')).toBe('plugin://weather-demo/');
    expect(messageFromPage({ neotime: { city: 'Kingston' } })).toEqual({ message: { city: 'Kingston' } });
    expect(messageFromPage({ other: 1 })).toBeNull();
    expect(messageFromPage('text')).toBeNull();
    expect(messageToPage(3)).toEqual({ neotime: 3 });
  });

  it('keeps only the newest log lines', () => {
    let lines = [{ id: 'a', line: '1' }];
    lines = appendLog(lines, { id: 'a', line: '2' }, 2);
    lines = appendLog(lines, { id: 'a', line: '3' }, 2);
    expect(lines.map((entry) => entry.line)).toEqual(['2', '3']);
  });
});
