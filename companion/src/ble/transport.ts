// The transport is the one seam between the app and Bluetooth, so screens never touch a BLE library directly.

import type { DfuLink } from '@/dfu/link';

export type DiscoveredWatch = {
  id: string;
  name: string;
  rssi: number | null;
  // True for a watch advertising a DFU service and no UART, which means it is
  // sitting in its bootloader waiting for firmware.
  bootloader?: boolean;
};

// A watch in its bootloader runs no firmware, so it answers none of the
// protocols the rest of the app speaks.
export type WatchMode = 'application' | 'bootloader';

export type ConnectionState = 'disconnected' | 'connecting' | 'connected';

// Which implementation is carrying the bytes, for the interface to report.
export type TransportKind = 'ble' | 'electron' | 'mock';

export type TransportListener = {
  onState?: (state: ConnectionState) => void;
  // What the watch turned out to be running, once connected.
  onMode?: (mode: WatchMode) => void;
  onLine?: (line: string) => void;
  onError?: (error: Error) => void;
};

export interface WatchTransport {
  readonly kind: TransportKind;

  // Scan for watches, by the UART service or by a bootloader's DFU service,
  // until stopScan is called.
  startScan(onFound: (watch: DiscoveredWatch) => void): Promise<void>;
  stopScan(): Promise<void>;
  connect(id: string): Promise<void>;
  disconnect(): Promise<void>;
  // Send raw text to the watch, splitting it into MTU-sized writes.
  write(text: string): Promise<void>;
  setListener(listener: TransportListener): void;
  // Raw GATT for a firmware update, which needs characteristics outside the
  // UART service. Null where the transport cannot reach them.
  dfuLink(): DfuLink | null;
  destroy(): void;
}
