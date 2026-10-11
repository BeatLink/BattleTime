import { describe, expect, it } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { createBackup, parseBackup, restoreBackup } from './backup';
import { FileService } from './file-service';
import { decodeWatchLine, encodeForWatch, type PhoneToWatchMessage } from './gadgetbridge';
import { readAlarms, readStatus, readWatchSettings, ReplyRouter, writeAlarms, writeWatchSettings } from './requests';
import { hourlySteps, readStepHistory } from './steps';
import { applyPreset, DEFAULT_THEME, hexToRgb565, rgb565ToHex, THEME_PRESETS, themeBytes, themeColours } from './theme';

// NeoTime's file service on a mock watch, as the provider opens it.
function fileService(watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true)): Promise<FileService> {
  return FileService.open(watch);
}

// The mock Gadgetbridge handler behind a reply router, as the provider wires it.
function requestChannel(): ReplyRouter {
  const mock = new MockGadgetbridge();
  const router: ReplyRouter = new ReplyRouter(async (message: PhoneToWatchMessage) => {
    for (const line of mock.handle(encodeForWatch(message)) ?? []) {
      const decoded = decodeWatchLine(line);
      if (decoded) router.push(decoded);
    }
  }, 1000);
  return router;
}

describe('reply router', () => {
  it('matches answers to requests by type and fails one on an error', async () => {
    const sent: PhoneToWatchMessage[] = [];
    const router = new ReplyRouter(async (message) => void sent.push(message), 1000);
    const settings = router.request({ t: 'settings' }, 'settings');
    const alarms = router.request({ t: 'alarm' }, 'alarm');
    expect(router.push({ t: 'alarm', d: [] })).toBe(true);
    await expect(alarms).resolves.toEqual({ t: 'alarm', d: [] });
    expect(router.push({ t: 'error', msg: 'Traceback\n  File "x"\nValueError: bad' })).toBe(true);
    await expect(settings).rejects.toThrow('ValueError: bad');
    expect(router.push({ t: 'findPhone', n: true })).toBe(false);
  });

  it('times out when the watch says nothing', async () => {
    const router = new ReplyRouter(async () => {}, 10);
    await expect(router.request({ t: 'settings' }, 'settings')).rejects.toThrow('did not reply');
  });
});

describe('settings and alarms', () => {
  it('reads and changes watch settings', async () => {
    const channel = requestChannel();
    const before = await readWatchSettings(channel);
    expect(before.faces.length).toBeGreaterThan(0);
    const after = await writeWatchSettings(channel, { brightness: 3, face: before.faces[1][0] });
    expect(after.brightness).toBe(3);
    expect(after.face).toBe(before.faces[1][0]);
  });

  it('replaces the alarm list and fills in defaults', async () => {
    const channel = requestChannel();
    expect(await readAlarms(channel)).toHaveLength(1);
    const written = await writeAlarms(channel, [{ h: 6, m: 5 }, { h: 25, m: 70, on: false, rep: 0xff }]);
    expect(written).toEqual([
      { h: 6, m: 5, on: true, rep: 0 },
      { h: 23, m: 59, on: false, rep: 0x7f },
    ]);
    await expect(writeAlarms(channel, new Array(13).fill({ h: 1, m: 1 }))).rejects.toThrow('at most 12');
  });
});

describe('steps', () => {
  it('folds 6 minute slots into hours', () => {
    const slots = new Array(240).fill(1);
    const hours = hourlySteps(slots);
    expect(hours).toHaveLength(24);
    expect(hours.every((count) => count === 10)).toBe(true);
  });

  it('reads a week, leaving days without a log empty', async () => {
    const history = await readStepHistory(requestChannel(), 7, new Date(2026, 9, 10));
    expect(history).toHaveLength(7);
    expect(history[0].date.getDate()).toBe(10);
    // The mock has no log on days divisible by five.
    expect(history[0].hours).toEqual([]);
    expect(history[1].total).toBeGreaterThan(0);
  });
});

describe('files', () => {
  it('lists, reads, writes, deletes and reports free space', async () => {
    const service = await fileService();
    expect((await service.list('/')).map((file) => file.name)).toEqual(['apps', 'resources', 'settings']);
    expect(new TextDecoder().decode(await service.read('/settings/theme.json'))).toBe('{"accent":"#00a0ff"}');

    // Larger than one 4 KB pacing step, so the write waits for the watch along the way.
    const big = Uint8Array.from({ length: 9000 }, (_value, index) => index & 0xff);
    const sent: number[] = [];
    await service.write('/resources/big.bin', big, (progress) => sent.push(progress.sent));
    expect(await service.read('/resources/big.bin')).toEqual(big);
    expect(sent.at(-1)).toBe(big.length);

    await service.remove('/settings/theme.json');
    await expect(service.read('/settings/theme.json')).rejects.toThrow('does not exist');
    expect((await service.storage()).free).toBeGreaterThan(0);
  });

  it('refuses paths that are relative or climb', async () => {
    const service = await fileService();
    await expect(service.read('settings')).rejects.toThrow('absolute path');
    await expect(service.read('/apps/../x')).rejects.toThrow('absolute path');
  });

  it('installs an app into its own folder and removes it again', async () => {
    const watch = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const service = await fileService(watch);
    await service.installApp('timer', new Uint8Array(300));
    expect(watch.fileService.installed.has('timer')).toBe(true);
    expect(watch.fileService.files.get('/apps/timer/app.ntapp')).toHaveLength(300);
    await service.removeApp('timer');
    expect(watch.fileService.installed.has('timer')).toBe(false);
    await expect(service.installApp('../evil', new Uint8Array(1))).rejects.toThrow('app id');
  });
});

describe('backup', () => {
  it('copies the settings and app state off and puts them back', async () => {
    const backup = await createBackup(await fileService());
    expect(Object.keys(backup.files).sort()).toEqual(['/apps/clock/state', '/settings/theme.json']);
    const copy = parseBackup(JSON.stringify(backup));

    const service = await fileService();
    await service.remove('/settings/theme.json');
    await restoreBackup(service, copy);
    expect(new TextDecoder().decode(await service.read('/settings/theme.json'))).toBe('{"accent":"#00a0ff"}');
    expect(() => parseBackup('{"hello": 1}')).toThrow('not a watch backup');
  });
});

describe('theme', () => {
  it('round trips the default theme and converts colours', () => {
    expect(themeBytes(themeColours(DEFAULT_THEME))).toEqual(DEFAULT_THEME);
    expect(rgb565ToHex(0xffff)).toBe('#ffffff');
    expect(rgb565ToHex(0x0000)).toBe('#000000');
    expect(hexToRgb565('#ff0000')).toBe(0xf800);
    expect(hexToRgb565('#0f0')).toBe(0x07e0);
    expect(hexToRgb565('nope')).toBeNull();
  });
});

describe('ascii lines', () => {
  it('escapes what the watch line editor would drop, and the watch reads it back', () => {
    const line = encodeForWatch({ t: 'notify', id: 1, title: '32\u00b0C caf\u00e9 \u{1f600}' });
    expect(/^[\x20-\x7e]*\n$/.test(line)).toBe(true);
    expect(line).toContain('32\\u00b0C caf\\u00e9 \\ud83d\\ude00');
    expect(JSON.parse(line.slice(3, -2)).title).toBe('32\u00b0C caf\u00e9 \u{1f600}');
  });
});

describe('status', () => {
  it('reads the battery and step count', async () => {
    const reply = await readStatus(requestChannel());
    expect(reply).toEqual({ t: 'status', battery: 76, mv: 3912, charging: false, steps: 4321 });
  });
});

describe('theme presets', () => {
  it('leaves the default theme unchanged and only touches the accent slots', () => {
    const colours = themeColours(DEFAULT_THEME);
    const byName = Object.fromEntries(THEME_PRESETS.map((preset) => [preset.name, preset]));
    expect(applyPreset(colours, byName.Default)).toEqual(colours);
    const forest = applyPreset(colours, byName.Forest);
    expect(forest.slice(0, 7)).toEqual(colours.slice(0, 7));
    expect(forest[7]).toBe(hexToRgb565('#2fbf71'));
  });
});
