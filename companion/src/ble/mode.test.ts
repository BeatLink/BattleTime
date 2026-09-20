import { describe, expect, it } from '@jest/globals';

import { QUICK_COMMANDS } from '@/protocol/diagnostics';
import { isTabVisible, visibleTabs } from '@/state/visible-tabs';

import { MockTransport } from './mock-transport';
import type { DiscoveredWatch, WatchMode } from './transport';

async function scan(transport: MockTransport): Promise<DiscoveredWatch[]> {
  const found: DiscoveredWatch[] = [];
  await transport.startScan((watch) => found.push(watch));
  await new Promise((resolve) => setTimeout(resolve, 700));
  await transport.stopScan();
  return found;
}

function watch(transport: MockTransport): { modes: WatchMode[]; lines: string[] } {
  const modes: WatchMode[] = [];
  const lines: string[] = [];
  transport.setListener({ onMode: (mode) => modes.push(mode), onLine: (line) => lines.push(line) });
  return { modes, lines };
}

describe('a watch in its bootloader', () => {
  it('turns up in a scan, marked as a bootloader', async () => {
    const found = await scan(new MockTransport());
    expect(found).toHaveLength(2);
    expect(found.filter((candidate) => candidate.bootloader)).toHaveLength(1);
    expect(found.find((candidate) => candidate.bootloader)?.name).toContain('DfuTarg');
  });

  it('reports bootloader mode once connected', async () => {
    const transport = new MockTransport();
    const seen = watch(transport);
    const found = await scan(transport);
    const bootloader = found.find((candidate) => candidate.bootloader);

    await transport.connect(bootloader!.id);

    expect(seen.modes).toEqual(['bootloader']);
    // Nothing greets you, because no firmware is running.
    expect(seen.lines).toEqual([]);
  });

  it('reports application mode for a watch running its firmware', async () => {
    const transport = new MockTransport();
    const seen = watch(transport);
    const found = await scan(transport);

    await transport.connect(found.find((candidate) => !candidate.bootloader)!.id);

    expect(seen.modes).toEqual(['application']);
    expect(seen.lines.join('')).toContain('mock watch connected');
  });

  it('still offers a DFU link, which is the point of connecting to it', async () => {
    const transport = new MockTransport();
    const found = await scan(transport);
    await transport.connect(found.find((candidate) => candidate.bootloader)!.id);
    expect(transport.dfuLink()).not.toBeNull();
  });
});

describe('the console quick commands', () => {
  it('import every module they use', () => {
    for (const command of QUICK_COMMANDS) {
      const imported = command.line.match(/^import ([\w, ]+);/)?.[1] ?? '';
      const modules = imported.split(',').map((name) => name.trim());
      for (const module of ['watch', 'gc', 'wasp', 'machine']) {
        if (new RegExp(`\\b${module}\\.`).test(command.line)) {
          expect(modules).toContain(module);
        }
      }
    }
  });

  it('gives each command a name the tool can be asked for', () => {
    const names = QUICK_COMMANDS.map((command) => command.label.toLowerCase().replace(/ /g, '-'));
    expect(new Set(names).size).toBe(names.length);
  });
});

describe('which tabs are offered', () => {
  it('offers only the connection screen and settings with no watch', () => {
    expect(visibleTabs('disconnected', 'application')).toEqual(['index', 'settings']);
    expect(visibleTabs('connecting', 'application')).toEqual(['index', 'settings']);
  });

  it('offers only the firmware update to a watch in its bootloader', () => {
    expect(visibleTabs('connected', 'bootloader')).toEqual(['index', 'settings', 'firmware']);
  });

  it('offers everything to a watch running its firmware', () => {
    const tabs = visibleTabs('connected', 'application');
    for (const tab of ['index', 'settings', 'firmware', 'packages', 'notifications', 'console']) {
      expect(tabs).toContain(tab);
    }
  });

  it('never offers the REPL or the package manager to a bootloader', () => {
    for (const tab of ['console', 'packages', 'notifications'] as const) {
      expect(isTabVisible(tab, 'connected', 'bootloader')).toBe(false);
      expect(isTabVisible(tab, 'disconnected', 'application')).toBe(false);
    }
  });

  it('never offers a firmware update without a watch to send it to', () => {
    expect(isTabVisible('firmware', 'disconnected', 'application')).toBe(false);
    expect(isTabVisible('firmware', 'connecting', 'bootloader')).toBe(false);
  });
});
