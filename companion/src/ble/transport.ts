// The transport is the one seam between the app and Bluetooth, so screens never touch a BLE library directly.

import type { DfuLink } from '@/dfu/link';

export type DiscoveredWatch = {
  id: string;
  name: string;
  rssi: number | null;
};

export type ConnectionState = 'disconnected' | 'connecting' | 'connected';

// Which implementation is carrying the bytes, for the interface to report.
export type TransportKind = 'ble' | 'electron' | 'mock';

export type TransportListener = {
  onState?: (state: ConnectionState) => void;
  onLine?: (line: string) => void;
  onError?: (error: Error) => void;
};

export interface WatchTransport {
  readonly kind: TransportKind;

  // Scan for watches advertising the Nordic UART Service until stopScan is called.
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
