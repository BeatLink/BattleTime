import { MockTransport } from './mock-transport';
import type { WatchTransport } from './transport';

// Web has no react-native-ble-plx, so it always gets the mock watch.
export function createTransport(): WatchTransport {
  return new MockTransport();
}

export const transportKind = 'mock' as const;
