// A debug report: what BattleTime's diagnostics say about the last restart and the crash before it,
// with the firmware version, the counters, the installed apps and the free storage, as one JSON
// object. Saving one tells the watch, which then forgets the crash and leaves its crash screen.
// The watch side is firmware/src/diagnostics.rs and services/crash_record in the BattleTime tree.

import type { DfuLink } from '@/dfu/link';
import type { WatchApp } from '@/protocol/packages';
import { DIAGNOSTIC_COUNTERS, BATTLETIME_DIAGNOSTICS, BATTLETIME_SERVICE, decodeDiagnostics } from '@/protocol/battletime';

// ============================================================================
// Where the last restart sits in the diagnostics
// ============================================================================

export const DEVICE_INFORMATION_SERVICE = '0000180a-0000-1000-8000-00805f9b34fb';
export const FIRMWARE_REVISION = '00002a26-0000-1000-8000-00805f9b34fb';

// The words after the counters: reset reasons, crash kind, eight detail words, then four of the open app's id.
const COUNTER_WORDS = DIAGNOSTIC_COUNTERS.length;
const DETAIL_WORDS = 8;
const OPEN_APP_WORDS = 4;

// The byte written to the diagnostics characteristic once a report holding the crash is saved.
export const DEBUG_REPORT_SAVED = 1;

// The nRF52832's RESETREAS bits, most telling first, as the watch names them.
const RESET_REASONS: [number, string][] = [
  [1 << 1, 'watchdog'],
  [1 << 3, 'lockup'],
  [1 << 2, 'software'],
  [1 << 0, 'reset pin'],
  [1 << 16, 'woke by pin'],
  [1 << 17, 'woke by comparator'],
  [1 << 18, 'debugger'],
  [1 << 19, 'woke by NFC'],
];

const hex = (word: number) => `0x${word.toString(16).padStart(8, '0')}`;

// ============================================================================
// The crash
// ============================================================================

export type Crash =
  | { kind: 'panic'; returnAddresses: string[] }
  | { kind: 'hardFault'; programCounter: string; linkRegister: string; faultStatus: string; faultAddress: string; stackOverflow: boolean }
  | { kind: 'outOfMemory'; requestedBytes: number; freeBytes: number; returnAddresses: string[] }
  | { kind: 'unknown'; code: number; details: string[] };

export type LastRestart = {
  resetReasons: string[];
  resetRegister: string;
  crash: Crash | null;
  // The id of the app open when it crashed, empty when none was or the firmware is too old to say.
  openApp: string;
};

// Return addresses with the empty slots left out.
const addresses = (words: number[]) => words.filter((word) => word !== 0).map(hex);

function crashOf(kind: number, details: number[]): Crash | null {
  switch (kind) {
    case 0:
      return null;
    case 1:
      return { kind: 'panic', returnAddresses: addresses(details) };
    case 2:
      return {
        kind: 'hardFault',
        programCounter: hex(details[0]),
        linkRegister: hex(details[1]),
        faultStatus: hex(details[2]),
        faultAddress: hex(details[3]),
        // Only the stack guard raises a memory management fault.
        stackOverflow: (details[2] & 0xff) !== 0,
      };
    case 3:
      return { kind: 'outOfMemory', requestedBytes: details[0], freeBytes: details[1], returnAddresses: addresses(details.slice(2)) };
    default:
      return { kind: 'unknown', code: kind, details: details.map(hex) };
  }
}

export function resetReasonNames(register: number): string[] {
  if (register === 0) {
    return ['power on'];
  }
  const known = RESET_REASONS.reduce((bits, [bit]) => bits | bit, 0);
  const names = RESET_REASONS.filter(([bit]) => (register & bit) !== 0).map(([, name]) => name);
  return (register & ~known) !== 0 ? [...names, 'unknown'] : names;
}

// The last restart from the diagnostics value, or null from a firmware that sends only counters.
export function decodeLastRestart(bytes: Uint8Array): LastRestart | null {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const word = (index: number) => view.getUint32((COUNTER_WORDS + index) * 4, true);
  const wordCount = Math.floor(bytes.length / 4) - COUNTER_WORDS;
  if (wordCount < 2 + DETAIL_WORDS) {
    return null;
  }
  const details = Array.from({ length: DETAIL_WORDS }, (_, index) => word(2 + index));
  let openApp = '';
  if (wordCount >= 2 + DETAIL_WORDS + OPEN_APP_WORDS) {
    const start = (COUNTER_WORDS + 2 + DETAIL_WORDS) * 4;
    const idBytes = bytes.slice(start, start + OPEN_APP_WORDS * 4);
    const end = idBytes.indexOf(0);
    openApp = new TextDecoder().decode(end < 0 ? idBytes : idBytes.slice(0, end));
  }
  return {
    resetReasons: resetReasonNames(word(0)),
    resetRegister: hex(word(0)),
    crash: crashOf(word(1), details),
    openApp,
  };
}

// What went wrong, in the words the watch's crash screen uses.
export function crashSummary(lastRestart: LastRestart | null): string {
  const crash = lastRestart?.crash;
  if (!crash) {
    return 'No crash before the last restart.';
  }
  const cause = {
    panic: 'a software error',
    hardFault: crash.kind === 'hardFault' && crash.stackOverflow ? 'out of stack space' : 'a processor fault',
    outOfMemory: 'out of memory',
    unknown: 'an unknown crash',
  }[crash.kind];
  return lastRestart.openApp ? `BattleTime stopped: ${cause} while ${lastRestart.openApp} was open.` : `BattleTime stopped: ${cause}.`;
}

// ============================================================================
// The report
// ============================================================================

export type DebugReport = {
  created: string;
  firmware: string | null;
  lastRestart: LastRestart | null;
  counters: Record<string, number>;
  apps: WatchApp[] | null;
  freeStorageBytes: number | null;
};

// Where the extra parts come from; each is optional, since a watch may not serve it.
export type ReportSources = {
  apps?: () => Promise<WatchApp[]>;
  freeStorageBytes?: () => Promise<number>;
  now?: Date;
};

// Reads a part, leaving it out rather than failing the whole report.
async function optional<Value>(read: (() => Promise<Value>) | undefined): Promise<Value | null> {
  if (!read) {
    return null;
  }
  try {
    return await read();
  } catch {
    return null;
  }
}

export async function readDebugReport(link: DfuLink, sources: ReportSources = {}): Promise<DebugReport> {
  const diagnostics = await link.read(BATTLETIME_SERVICE, BATTLETIME_DIAGNOSTICS);
  const firmwareBytes = await optional(() => link.read(DEVICE_INFORMATION_SERVICE, FIRMWARE_REVISION));
  const counters: Record<string, number> = {};
  for (const counter of decodeDiagnostics(diagnostics).slice(0, COUNTER_WORDS)) {
    counters[counter.key] = counter.value;
  }
  return {
    created: (sources.now ?? new Date()).toISOString(),
    firmware: firmwareBytes?.length ? new TextDecoder().decode(firmwareBytes) : null,
    lastRestart: decodeLastRestart(diagnostics),
    counters,
    apps: await optional(sources.apps),
    freeStorageBytes: await optional(sources.freeStorageBytes),
  };
}

// Tells the watch the crash is saved, so it forgets it and leaves its crash screen.
export async function markDebugReportSaved(link: DfuLink): Promise<void> {
  await link.write(BATTLETIME_SERVICE, BATTLETIME_DIAGNOSTICS, Uint8Array.from([DEBUG_REPORT_SAVED]), 'request');
}

export function debugReportFileName(report: DebugReport): string {
  return `battletime-debug-${report.created.slice(0, 19).replace(/:/g, '-')}.json`;
}

// Reads a report, hands it to `save` as a file name and JSON text, then tells the watch when it held a crash.
export async function saveDebugReport(
  link: DfuLink,
  sources: ReportSources,
  save: (fileName: string, json: string) => Promise<void>,
): Promise<DebugReport> {
  const report = await readDebugReport(link, sources);
  await save(debugReportFileName(report), `${JSON.stringify(report, null, 2)}\n`);
  if (report.lastRestart?.crash) {
    await markDebugReportSaved(link);
  }
  return report;
}
