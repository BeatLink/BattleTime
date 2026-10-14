import { describe, expect, it } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { createBackup, parseBackup, restoreBackup } from './backup';
import { FileService } from './file-service';
import { decodeWatchLine, encodeForWatch, type PhoneToWatchMessage } from './gadgetbridge';
import { readCapabilities } from './neotime';
import {
  fittedRecoverySlot,
  moveSlots,
  readAlarms,
  readSlots,
  readStatus,
  readWatchSettings,
  ReplyRouter,
  writeAlarms,
  writeWatchSettings,
} from './requests';
import { hourlySteps, readStepHistory } from './steps';
import { changedColors, normaliseHex } from './theme';

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
    expect(before.themes).toContain('light');
    const after = await writeWatchSettings(channel, { brightness: 3, theme: 'light' });
    expect(after.brightness).toBe(3);
    expect(after.theme).toBe('light');
    expect(after.theme_colors.background).toBe('#ffffff');
  });

  it('turns a changed colour into a custom theme, and refuses a bad value whole', async () => {
    const channel = requestChannel();
    const custom = await writeWatchSettings(channel, { theme_colors: { accent: '#ff8000' } });
    expect(custom.theme).toBe('custom');
    expect(custom.theme_colors.accent).toBe('#ff8000');
    await expect(writeWatchSettings(channel, { brightness: 2, theme: 'neon' })).rejects.toThrow('unknown theme');
    expect((await readWatchSettings(channel)).brightness).toBe(custom.brightness);
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

    // Many notifications long, so the read paces and the write sends many pieces.
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

  it('is offered only on a NeoTime watch that serves it', async () => {
    const neotime = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    expect((await readCapabilities(neotime)).files).toBe(true);
    const wasp = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'wasp-os', true);
    expect((await readCapabilities(wasp)).files).toBe(false);
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
  it('normalises colours and sends only the ones that changed', () => {
    expect(normaliseHex('#0F0')).toBe('#00ff00');
    expect(normaliseHex('ff8000')).toBe('#ff8000');
    expect(normaliseHex('nope')).toBeNull();
    const base = {
      background: '#000000', surface: '#292c31', text: '#ffffff', muted: '#8c929c',
      accent: '#00aaa5', success: '#39be5a', warning: '#f7aa18', danger: '#e74539',
    };
    expect(changedColors(base, { ...base, accent: '#FF8000', text: '#FFFFFF' })).toEqual({ accent: '#FF8000' });
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
    expect(reply).toEqual({ t: 'status', bat: 76, mv: 3912, charging: false, steps: 4321 });
  });
});

describe('slots', () => {
  it('reads where the slots are', async () => {
    const report = await readSlots(requestChannel());
    expect(report.update).toEqual([0, 392 * 1024]);
    expect(report.recovery).toEqual([0x65000, 388 * 1024]);
    expect(report.restart).toBe(false);
  });

  it('moves a slot and says the watch restarts', async () => {
    const channel = requestChannel();
    const report = await moveSlots(channel, { recovery: [0xc6000, 172 * 1024] });
    expect(report.recovery).toEqual([0xc6000, 172 * 1024]);
    expect(report.restart).toBe(true);
    expect((await readSlots(channel)).recovery).toEqual([0xc6000, 172 * 1024]);
  });

  it('turns a refusal into an error that says why', async () => {
    await expect(moveSlots(requestChannel(), { recovery: [0x1000, 172 * 1024] })).rejects.toThrow(
      'The watch refused: the slots overlap.',
    );
  });

  it('fits the recovery slot to its image in whole pages, never under 64 KB', () => {
    const report = {
      t: 'slots' as const,
      update: [0, 392 * 1024] as [number, number],
      recovery: [0x65000, 388 * 1024] as [number, number],
      area: 0x100000,
      firmware: 388 * 1024,
      image: 171428,
      restart: false,
    };
    expect(fittedRecoverySlot(report)).toEqual([0x65000, 168 * 1024]);
    expect(fittedRecoverySlot({ ...report, image: 0 })).toEqual([0x65000, 64 * 1024]);
  });
});
