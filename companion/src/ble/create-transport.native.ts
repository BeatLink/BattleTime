import { MockTransport } from './mock-transport';
import type { TransportKind, WatchTransport } from './transport';

// Use real Bluetooth when the native module is present and fall back to the
// mock inside Expo Go.
export function createTransport(): WatchTransport {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { BleManager } = require('react-native-ble-plx') as typeof import('react-native-ble-plx');
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { BleTransport } = require('./ble-transport') as typeof import('./ble-transport');
    return new BleTransport(new BleManager());
  } catch {
    return new MockTransport();
  }
}

// Which transport this environment will get, without building one. The
// require is cached, so asking is cheap and has no side effects of its own.
export function detectTransportKind(): TransportKind {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    require('react-native-ble-plx');
    return 'ble';
  } catch {
    return 'mock';
  }
}
