// Types for node/ble.js, so the command line tool can use it from TypeScript.

import type { DiscoveredWatch, WatchMode } from '@/ble/transport';
import type { WriteMode } from '@/dfu/link';

// A scan also reports the services a device advertises, which the command
// line tool uses to pick out watches.
export type ScannedDevice = DiscoveredWatch & { uuids: string[]; bootloader: boolean };

export type WatchBleEvent = {
  device: ScannedDevice;
  line: string;
  state: 'disconnected' | 'connecting' | 'connected';
  mode: WatchMode;
  error: string;
  notify: { key: string; value: number[] };
};

export declare class WatchBle {
  constructor(emit: <K extends keyof WatchBleEvent>(name: K, payload: WatchBleEvent[K]) => void);
  startScan(): Promise<void>;
  stopScan(): Promise<void>;
  connect(id: string, uart?: boolean): Promise<void>;
  reconnect(id: string): Promise<void>;
  disconnect(): Promise<void>;
  write(text: string): Promise<void>;
  writeBytes(data: Uint8Array | number[]): Promise<void>;
  gattHas(service: string, characteristic: string): Promise<boolean>;
  gattRead(service: string, characteristic: string): Promise<number[]>;
  gattWrite(
    service: string,
    characteristic: string,
    data: number[],
    mode: WriteMode,
  ): Promise<void>;
  gattSubscribe(service: string, characteristic: string): Promise<void>;
  gattUnsubscribe(service: string, characteristic: string): Promise<void>;
  destroy(): Promise<void>;
}

/** Calls `onDropped` once when BlueZ says the device is no longer connected, however the link was made. */
export declare function watchForDisconnect(
  device: { on(event: 'disconnect', listener: () => void): unknown; helper?: { on(event: string, listener: (changed: unknown) => void): unknown } },
  onDropped: () => void,
): void;
