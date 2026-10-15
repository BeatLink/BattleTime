import { describe, expect, it } from '@jest/globals';

import { concat, u32le } from '@/dfu/bytes';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import {
  crashSummary,
  debugReportFileName,
  decodeLastRestart,
  readDebugReport,
  resetReasonNames,
  saveDebugReport,
} from './debug-report';
import { decodeDiagnostics } from './battletime';

const COUNTERS = [10, 1, 1, 1, 5, 4, 2, 1];

function diagnostics(...lastRestart: number[]): Uint8Array {
  return concat(...[...COUNTERS, ...lastRestart].map(u32le));
}

describe('the last restart', () => {
  it('names each reset reason, power on when there is none, and unknown bits', () => {
    expect(resetReasonNames(0)).toEqual(['power on']);
    expect(resetReasonNames(0b111)).toEqual(['watchdog', 'software', 'reset pin']);
    expect(resetReasonNames((1 << 3) | (1 << 16) | (1 << 17) | (1 << 18) | (1 << 19))).toEqual([
      'lockup',
      'woke by pin',
      'woke by comparator',
      'debugger',
      'woke by NFC',
    ]);
    expect(resetReasonNames((1 << 1) | (1 << 30))).toEqual(['watchdog', 'unknown']);
  });

  it('is missing from a firmware that sends only the counters', () => {
    expect(decodeLastRestart(diagnostics())).toBeNull();
    expect(decodeDiagnostics(diagnostics(2, 0))).toHaveLength(8);
  });

  it('reads a restart without a crash', () => {
    const lastRestart = decodeLastRestart(diagnostics(4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));

    expect(lastRestart).toEqual({ resetReasons: ['software'], resetRegister: '0x00000004', crash: null, openApp: '' });
    expect(crashSummary(lastRestart)).toBe('No crash before the last restart.');
    expect(crashSummary(null)).toBe('No crash before the last restart.');
  });

  it('reads a panic with its return addresses and the open app', () => {
    const app = new TextEncoder().encode('stopwatch\0\0\0\0\0\0\0');
    const words = Array.from({ length: 4 }, (_, index) => new DataView(app.buffer).getUint32(index * 4, true));
    const lastRestart = decodeLastRestart(diagnostics(2, 1, 0x5101, 0x1a2b5, 0, 0, 0, 0, 0, 0, ...words));

    expect(lastRestart?.crash).toEqual({ kind: 'panic', returnAddresses: ['0x00005101', '0x0001a2b5'] });
    expect(lastRestart?.openApp).toBe('stopwatch');
    expect(crashSummary(lastRestart)).toBe('BattleTime stopped: a software error while stopwatch was open.');
  });

  it('reads a fault, and tells a stack overflow by its memory management bits', () => {
    const fault = decodeLastRestart(diagnostics(2, 2, 0x1a2b4, 0x10f03, 0x10000, 0xe0000000, 0, 0, 0, 0));
    const overflow = decodeLastRestart(diagnostics(2, 2, 0x1a2b4, 0x10f03, 0x12, 0x2000d000, 0, 0, 0, 0));

    expect(fault?.crash).toEqual({
      kind: 'hardFault',
      programCounter: '0x0001a2b4',
      linkRegister: '0x00010f03',
      faultStatus: '0x00010000',
      faultAddress: '0xe0000000',
      stackOverflow: false,
    });
    expect(fault?.openApp).toBe('');
    expect(crashSummary(fault)).toBe('BattleTime stopped: a processor fault.');
    expect(crashSummary(overflow)).toBe('BattleTime stopped: out of stack space.');
  });

  it('reads running out of memory and a kind it does not know', () => {
    const memory = decodeLastRestart(diagnostics(2, 3, 2048, 612, 0x5101, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    const unknown = decodeLastRestart(diagnostics(2, 9, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));

    expect(memory?.crash).toEqual({ kind: 'outOfMemory', requestedBytes: 2048, freeBytes: 612, returnAddresses: ['0x00005101'] });
    expect(crashSummary(memory)).toBe('BattleTime stopped: out of memory.');
    expect(unknown?.crash).toMatchObject({ kind: 'unknown', code: 9 });
    expect(crashSummary(unknown)).toBe('BattleTime stopped: an unknown crash.');
  });
});

describe('a debug report', () => {
  const now = new Date('2026-10-12T08:30:15.000Z');

  it('holds the firmware, the counters, the crash, the apps and the free storage', async () => {
    const watch = new MockLegacyDfuTarget();
    const apps = [{ id: 'alarm', name: 'Alarm', kind: 'app' as const, enabled: true, bytes: 6100, need: 9000, group: 'time' }];

    const report = await readDebugReport(watch, { apps: async () => apps, freeStorageBytes: async () => 123456, now });

    expect(report.created).toBe('2026-10-12T08:30:15.000Z');
    expect(report.firmware).toBe('0.1.0+mock');
    expect(report.counters.shellLoops).toBe(1234);
    expect(Object.keys(report.counters)).toHaveLength(8);
    expect(report.lastRestart?.crash?.kind).toBe('outOfMemory');
    expect(report.lastRestart?.openApp).toBe('alarm');
    expect(report.apps).toEqual(apps);
    expect(report.freeStorageBytes).toBe(123456);
    expect(debugReportFileName(report)).toBe('battletime-debug-2026-10-12T08-30-15.json');
  });

  it('leaves out what the watch does not serve or fails to answer', async () => {
    const watch = new MockLegacyDfuTarget();

    const report = await readDebugReport(watch, {
      freeStorageBytes: async () => {
        throw new Error('no file service');
      },
    });

    expect(report.apps).toBeNull();
    expect(report.freeStorageBytes).toBeNull();
  });

  it('is saved, then the watch is told so it forgets the crash', async () => {
    const watch = new MockLegacyDfuTarget();
    const saved: [string, string][] = [];

    await saveDebugReport(watch, { now }, async (name, json) => {
      saved.push([name, json]);
    });

    expect(saved[0][0]).toBe('battletime-debug-2026-10-12T08-30-15.json');
    expect(JSON.parse(saved[0][1]).lastRestart.crash.requestedBytes).toBe(2048);
    expect(watch.debugReportsSaved).toBe(1);
    expect(watch.crashPending).toBe(false);
  });

  it('tells the watch nothing when there was no crash, or the save failed', async () => {
    const calm = new MockLegacyDfuTarget();
    calm.lastRestartWords = [4, ...new Array(13).fill(0)];
    const failing = new MockLegacyDfuTarget();

    await saveDebugReport(calm, {}, async () => {});
    await expect(
      saveDebugReport(failing, {}, async () => {
        throw new Error('disk full');
      }),
    ).rejects.toThrow('disk full');

    expect(calm.debugReportsSaved).toBe(0);
    expect(failing.debugReportsSaved).toBe(0);
  });
});
