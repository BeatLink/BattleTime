import { desktopBridge, ElectronTransport } from './electron-transport';
import { MockTransport } from './mock-transport';
import type { TransportKind, WatchTransport } from './transport';

// In the desktop app the preload bridge is there and Bluetooth is real. In a
// browser there is no BLE, so the mock watch stands in.
export function createTransport(): WatchTransport {
  const bridge = desktopBridge();
  return bridge ? new ElectronTransport(bridge) : new MockTransport();
}

// Which transport this environment will get, without building one.
export function detectTransportKind(): TransportKind {
  return desktopBridge() ? 'electron' : 'mock';
}
