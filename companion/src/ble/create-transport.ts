import { MockTransport } from './mock-transport';
import { bluetoothBridge, TauriTransport } from './tauri-transport';
import type { TransportKind, WatchTransport } from './transport';

// In the app the bridge is there and Bluetooth is real. In a plain browser there is none, so the mock watch stands in.
export function createTransport(): WatchTransport {
  const bridge = bluetoothBridge();
  return bridge ? new TauriTransport(bridge) : new MockTransport();
}

// Which transport this environment will get, without building one.
export function detectTransportKind(): TransportKind {
  return bluetoothBridge() ? 'tauri' : 'mock';
}
