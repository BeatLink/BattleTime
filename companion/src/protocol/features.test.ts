import { describe, expect, it } from '@jest/globals';

import { MockGadgetbridge } from '@/ble/mock-gadgetbridge';
import { MockPackages } from '@/ble/mock-packages';
import { ReplyQueue } from '@/state/reply-queue';

import { createBackup, parseBackup, restoreBackup } from './backup';
import { encodeSetTime } from './clock';
import { deleteFile, listDirectory, readFile, readMemory } from './files';
import { decodeWatchLine, encodeForWatch, type PhoneToWatchMessage } from './gadgetbridge';
import type { PackageReply } from './packages';
import { readAlarms, readWatchSettings, ReplyRouter, writeAlarms, writeWatchSettings } from './requests';
import { hourlySteps, readStepHistory } from './steps';
import { DEFAULT_THEME, hexToRgb565, rgb565ToHex, themeBytes, themeColours } from './theme';
import type { PackageChannel } from './transfer';

// The mock package manager behind the same channel the app uses.
function packageChannel(mock = new MockPackages()): PackageChannel {
  const replies = new ReplyQueue<PackageReply>(1000);
  const deliver = (lines: string[] | null) => lines?.forEach((line) => replies.push(JSON.parse(line)));
  return {
    send: async (text) => deliver(mock.handle(text)),
    sendBytes: async (data) => deliver(mock.handleBytes(data)),
    next: () => replies.next(),
  };
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
  it('lists, reads, deletes and reports memory', async () => {
    const channel = packageChannel();
    const top = await listDirectory(channel, '');
    expect(top.map((file) => file.name)).toEqual(['alarms.txt', 'settings.json']);
    const data = await readFile(channel, 'alarms.txt');
    expect(new TextDecoder().decode(data)).toBe('7,30,159;');
    await deleteFile(channel, 'alarms.txt');
    await expect(readFile(channel, 'alarms.txt')).rejects.toThrow('no such file');
    expect((await readMemory(channel)).free).toBeGreaterThan(0);
  });

  it('refuses paths that climb out of the flash', async () => {
    await expect(readFile(packageChannel(), '../etc')).rejects.toThrow('Unsafe path');
    await expect(readFile(packageChannel(), '/main.py')).rejects.toThrow('Unsafe path');
  });
});

describe('backup', () => {
  it('copies the watch data off and puts it back', async () => {
    const backup = await createBackup(packageChannel());
    expect(Object.keys(backup.files).sort()).toEqual(['alarms.txt', 'settings.json']);
    const copy = parseBackup(JSON.stringify(backup));

    const fresh = new MockPackages();
    const channel = packageChannel(fresh);
    await deleteFile(channel, 'alarms.txt');
    await restoreBackup(channel, copy);
    expect(new TextDecoder().decode(await readFile(channel, 'alarms.txt'))).toBe('7,30,159;');
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

describe('clock', () => {
  it('sets the watch to local time', () => {
    expect(encodeSetTime(new Date(2026, 9, 10, 7, 5, 9))).toBe(
      'import watch; watch.rtc.set_localtime((2026, 10, 10, 7, 5, 9))\r\n',
    );
  });
});

describe('ascii lines', () => {
  it('escapes what the watch line editor would drop, and the watch reads it back', () => {
    const line = encodeForWatch({ t: 'notify', id: 1, title: '32\u00b0C caf\u00e9 \u{1f600}' });
    expect(/^[\x20-\x7e]*\r\n$/.test(line)).toBe(true);
    expect(line).toContain('32\\u00b0C caf\\u00e9 \\ud83d\\ude00');
    expect(JSON.parse(line.slice(3, -3)).title).toBe('32\u00b0C caf\u00e9 \u{1f600}');
  });
});
