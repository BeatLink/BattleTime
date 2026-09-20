import { MockTransport } from './mock-transport';
import type { WatchTransport } from './transport';

export let transportKind: 'ble' | 'mock' = 'mock';

// Use real Bluetooth when the native module is present and fall back to the mock inside Expo Go.
export function createTransport(): WatchTransport {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { BleManager } = require('react-native-ble-plx') as typeof import('react-native-ble-plx');
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { BleTransport } = require('./ble-transport') as typeof import('./ble-transport');
    const transport = new BleTransport(new BleManager());
    transportKind = 'ble';
    return transport;
  } catch {
    transportKind = 'mock';
    return new MockTransport();
  }
}
