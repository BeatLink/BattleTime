/**
 * @jest-environment jsdom
 */
import '@/test-support/jsdom-text-encoding';

import { afterEach, beforeEach, describe, expect, it, jest } from '@jest/globals';
import { zipSync } from 'fflate';
import { render } from 'preact';
import { act } from 'preact/test-utils';

import { utf8Encode } from '@/ble/encoding';
import { storeKey, type StoreEntry } from '@/services/app-store';
import { fetchOnlinePlugin, listOnlinePlugins, type OnlinePlugin } from '@/services/github-store';
import type { PluginBridge, PluginStatus } from '@/services/plugins';

import StoreScreen from './store';

jest.mock('@/hooks/use-watch', () => ({
  useWatch: () => ({
    files: async () => ({}),
    packages: async () => ({ listing: async () => ({ apps: [] }), apps: async () => [] }),
    watchReady: true,
    settings: { githubRepo: 'a/b', githubToken: '' },
  }),
}));

jest.mock('@/services/github-store', () => ({
  ...jest.requireActual<object>('@/services/github-store'),
  listOnlinePlugins: jest.fn(),
  fetchOnlinePlugin: jest.fn(),
}));

// ============================================================================
// Helpers
// ============================================================================

const manifest = {
  id: 'buzz-back',
  name: 'Buzz back',
  version: '1.1.0',
  description: 'Buzzes back.',
  watch: [],
  permissions: { messages: [], hosts: [] },
};
const pluginFile = zipSync({ 'manifest.json': utf8Encode(JSON.stringify(manifest)) });

const entry: StoreEntry = {
  key: storeKey('plugin', 'buzz-back'),
  kind: 'plugin',
  id: 'buzz-back',
  name: 'Buzz back',
  description: 'Buzzes back.',
  group: '',
  version: '1.1.0',
  watch: [],
  bytes: pluginFile.length,
  source: { type: 'github', repo: 'a/b', release: 'v1' },
};
const online: OnlinePlugin = {
  entry,
  file: { name: 'buzz-back.ntplugin', url: 'https://api/assets/22', sizeBytes: 1 },
  sha256: '',
};

// A desktop app's plugins that holds whatever is installed into it.
function fakeBridge(): { bridge: PluginBridge; installed: number[][] } {
  const installed: number[][] = [];
  const statuses: PluginStatus[] = [];
  const bridge = {
    list: async () => ({ ok: true, value: statuses }),
    install: async (data: number[]) => {
      installed.push(data);
      statuses.push({ manifest, enabled: true, running: true });
      return { ok: true, value: statuses[0] };
    },
    installOnWatch: async () => ({ ok: true, value: [] }),
  } as unknown as PluginBridge;
  return { bridge, installed };
}

// Lets the screen's reads and the effects they set off finish.
async function settle(): Promise<void> {
  for (let round = 0; round < 10; round++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

async function draw(): Promise<HTMLElement> {
  const host = document.createElement('div');
  document.body.appendChild(host);
  await act(async () => {
    render(<StoreScreen />, host);
  });
  await settle();
  return host;
}

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text));
  if (!found) throw new Error(`No button with "${text}" in: ${host.textContent}`);
  return found;
}

beforeEach(() => {
  localStorage.clear();
  jest.mocked(listOnlinePlugins).mockResolvedValue([online]);
  jest.mocked(fetchOnlinePlugin).mockResolvedValue({ entry, data: pluginFile });
});

afterEach(() => {
  document.body.innerHTML = '';
  delete (globalThis as { battletime?: unknown }).battletime;
});

// ============================================================================
// Tests
// ============================================================================

describe('the store screen', () => {
  it("lists the repository's plugins online, and installing one downloads, keeps and installs it", async () => {
    const { bridge, installed } = fakeBridge();
    (globalThis as { battletime?: unknown }).battletime = { plugins: bridge };
    const host = await draw();

    expect(listOnlinePlugins).toHaveBeenCalledWith('a/b', { token: undefined });
    expect(host.textContent).toContain('Plugins');
    expect(button(host, 'Buzz back').textContent).toContain('Not installed');

    await act(async () => button(host, 'Buzz back').click());
    expect(host.textContent).not.toContain('Remove from the store');
    await act(async () => button(host, 'Install').click());
    await settle();

    expect(fetchOnlinePlugin).toHaveBeenCalledWith(online, { token: undefined });
    expect(installed).toEqual([Array.from(pluginFile)]);
    expect(host.textContent).toContain('Installed Buzz back.');
    // Kept now at the same version, it is the store's own entry and can be taken out again.
    expect(host.textContent).toContain('Remove from the store');
  });

  it('says when the plugins cannot be listed, and still shows the bundled apps', async () => {
    jest.mocked(listOnlinePlugins).mockRejectedValue(new Error('GitHub answered 403'));
    const host = await draw();

    expect(host.textContent).toContain('Could not list the plugins online: GitHub answered 403');
    expect(host.textContent).toContain('Apps');
  });
});
