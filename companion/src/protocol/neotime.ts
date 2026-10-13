// The GATT services NeoTime serves besides DFU: Current Time, its own diagnostics and the BLEFS file service.
// The watch side lives in firmware/src/main.rs, clock.rs and diagnostics.rs in the NeoTime tree; keep them in step.

import type { DfuLink } from '@/dfu/link';
import { LEGACY_CONTROL_POINT, LEGACY_DFU_SERVICE } from '@/dfu/uuids';
import { NUS_RX, NUS_SERVICE } from '@/ble/uuids';

// ============================================================================
// Service and characteristic UUIDs
// ============================================================================

export const CURRENT_TIME_SERVICE = '00001805-0000-1000-8000-00805f9b34fb';
export const CURRENT_TIME = '00002a2b-0000-1000-8000-00805f9b34fb';

export const BATTERY_SERVICE = '0000180f-0000-1000-8000-00805f9b34fb';
export const BATTERY_LEVEL = '00002a19-0000-1000-8000-00805f9b34fb';

// NeoTime's own services share this base, which ends in "NeoTim" in ASCII.
export const NEOTIME_SERVICE = '6e540001-0000-4000-8000-4e656f54696d';
export const NEOTIME_DIAGNOSTICS = '6e540002-0000-4000-8000-4e656f54696d';
// The packages characteristic: JSON commands that install, list and remove apps and the shared library.
export const NEOTIME_PACKAGES = '6e540003-0000-4000-8000-4e656f54696d';

// Adafruit's BLE file transfer under InfiniTime's service UUID; see file-service.ts.
export const FILE_TRANSFER_SERVICE = '0000febb-0000-1000-8000-00805f9b34fb';
export const FILE_TRANSFER_VERSION = 'adaf0100-4669-6c65-5472-616e73666572';
export const FILE_TRANSFER = 'adaf0200-4669-6c65-5472-616e73666572';

// NeoTime's advertisement has no room for its own service, so a scan recognises it by this name.
export const NEOTIME_NAME = 'NeoTime';

// ============================================================================
// What the connected watch can do
// ============================================================================

// Which services the watch serves, so each screen is offered only where it works.
export type WatchCapabilities = {
  dfu: boolean;
  time: boolean;
  battery: boolean;
  diagnostics: boolean;
  // The BLEFS file service, for files, backups and installing apps.
  files: boolean;
  // The packages characteristic, for installing apps, faces and the shared library.
  packages: boolean;
  // The Nordic UART message channel, for notifications, music, weather, settings and the rest.
  uart: boolean;
};

export const NO_CAPABILITIES: WatchCapabilities = {
  dfu: false,
  time: false,
  battery: false,
  diagnostics: false,
  files: false,
  packages: false,
  uart: false,
};

export async function readCapabilities(link: DfuLink): Promise<WatchCapabilities> {
  const [dfu, time, battery, diagnostics, files, packages, uart] = await Promise.all([
    link.hasCharacteristic(LEGACY_DFU_SERVICE, LEGACY_CONTROL_POINT),
    link.hasCharacteristic(CURRENT_TIME_SERVICE, CURRENT_TIME),
    link.hasCharacteristic(BATTERY_SERVICE, BATTERY_LEVEL),
    link.hasCharacteristic(NEOTIME_SERVICE, NEOTIME_DIAGNOSTICS),
    link.hasCharacteristic(FILE_TRANSFER_SERVICE, FILE_TRANSFER),
    link.hasCharacteristic(NEOTIME_SERVICE, NEOTIME_PACKAGES),
    link.hasCharacteristic(NUS_SERVICE, NUS_RX),
  ]);
  return { dfu, time, battery, diagnostics, files, packages, uart };
}

// ============================================================================
// Current Time
// ============================================================================

// The ten-byte Current Time value in local time: year, month, day, hour, minute, second, weekday, fraction, reason.
export function encodeCurrentTime(now: Date = new Date()): Uint8Array {
  const year = now.getFullYear();
  const weekday = now.getDay() === 0 ? 7 : now.getDay();
  const fraction = Math.floor((now.getMilliseconds() * 256) / 1000);
  return Uint8Array.from([
    year & 0xff,
    year >> 8,
    now.getMonth() + 1,
    now.getDate(),
    now.getHours(),
    now.getMinutes(),
    now.getSeconds(),
    weekday,
    fraction,
    1,
  ]);
}

export async function setWatchTime(link: DfuLink, now: Date = new Date()): Promise<void> {
  await link.write(CURRENT_TIME_SERVICE, CURRENT_TIME, encodeCurrentTime(now), 'request');
}

// ============================================================================
// Battery
// ============================================================================

export async function readBatteryLevel(link: DfuLink): Promise<number> {
  const value = await link.read(BATTERY_SERVICE, BATTERY_LEVEL);
  return value[0] ?? 0;
}

// ============================================================================
// Diagnostics
// ============================================================================

// The counters in the order the firmware's `Counter` enum lists them.
export const DIAGNOSTIC_COUNTERS = [
  { key: 'shellLoops', label: 'Shell loops' },
  { key: 'shellLastWake', label: 'Last wake (1 touch, 2 button, 3 timer, 4 system)' },
  { key: 'screenLit', label: 'Screen lit' },
  { key: 'touchState', label: 'Touch panel (1 started, 2 failed)' },
  { key: 'touchReports', label: 'Touch reports read' },
  { key: 'touchesSent', label: 'Touches sent to the shell' },
  { key: 'buttonPresses', label: 'Button presses' },
  { key: 'accelerometerState', label: 'Accelerometer (1 started, 2 failed, 3 read failed, 4 waiting for the shared library)' },
] as const;

export type DiagnosticCounter = { key: string; label: string; value: number };

// Little-endian 32-bit counters; the words on the last restart that follow them are read by debug-report.ts.
export function decodeDiagnostics(bytes: Uint8Array): DiagnosticCounter[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const sent = DIAGNOSTIC_COUNTERS.slice(0, Math.floor(bytes.length / 4));
  return sent.map(({ key, label }, index) => ({ key, label, value: view.getUint32(index * 4, true) }));
}

export async function readDiagnostics(link: DfuLink): Promise<DiagnosticCounter[]> {
  return decodeDiagnostics(await link.read(NEOTIME_SERVICE, NEOTIME_DIAGNOSTICS));
}
