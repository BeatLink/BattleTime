import { describe, expect, it } from '@jest/globals';

import { NO_CAPABILITIES, type WatchCapabilities } from '@/protocol/neotime';
import { isTabVisible, visibleTabs } from '@/state/visible-tabs';

import { remoteTapLine } from './mock-gadgetbridge';
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

describe('simulated remote taps', () => {
  it('arrive as the line the watch sends, only while a watch running firmware is connected', async () => {
    const transport = new MockTransport();
    const seen = watch(transport);
    transport.simulateWatchLine(remoteTapLine(2));
    expect(seen.lines).toEqual([]);

    await transport.connect('AA:BB:CC:DD:EE:00');
    transport.simulateWatchLine(remoteTapLine(2));
    expect(seen.lines.at(-1)).toBe('{"t":"app","ch":"remote","d":{"button":2}}\r\n');

    await transport.connect('AA:BB:CC:DD:EE:11');
    const before = seen.lines.length;
    transport.simulateWatchLine(remoteTapLine(0));
    expect(seen.lines).toHaveLength(before);
  });
});

describe('the mock watches', () => {
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

  it('reports application mode for a NeoTime watch, which serves its own services', async () => {
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

const NEOTIME: WatchCapabilities = { dfu: true, time: true, battery: false, diagnostics: true, files: false, packages: false, uart: false };
const WITH_UART: WatchCapabilities = { ...NEOTIME, uart: true };

describe('which tabs are offered', () => {
  it('offers only the connection screen and settings with no watch', () => {
    expect(visibleTabs('disconnected', 'application', NO_CAPABILITIES)).toEqual(['index', 'settings']);
    expect(visibleTabs('connecting', 'application', NO_CAPABILITIES)).toEqual(['index', 'settings']);
  });

  it('offers only the firmware update to a watch in its bootloader', () => {
    expect(visibleTabs('connected', 'bootloader', NO_CAPABILITIES)).toEqual(['index', 'settings', 'firmware']);
  });

  it('offers the firmware update to NeoTime without a UART channel, and nothing that needs one', () => {
    expect(visibleTabs('connected', 'application', NEOTIME)).toEqual(['index', 'settings', 'firmware']);
  });

  it('offers everything to a watch with a UART channel', () => {
    const tabs = visibleTabs('connected', 'application', WITH_UART);
    for (const tab of ['index', 'settings', 'firmware', 'notifications', 'console']) {
      expect(tabs).toContain(tab);
    }
  });

  it('never offers the message tabs to a bootloader', () => {
    for (const tab of ['console', 'notifications'] as const) {
      expect(isTabVisible(tab, 'connected', 'bootloader', WITH_UART)).toBe(false);
      expect(isTabVisible(tab, 'disconnected', 'application', WITH_UART)).toBe(false);
    }
  });

  it('never offers a firmware update without a watch to send it to', () => {
    expect(isTabVisible('firmware', 'disconnected', 'application', NEOTIME)).toBe(false);
    expect(isTabVisible('firmware', 'connecting', 'bootloader', NEOTIME)).toBe(false);
  });
});
