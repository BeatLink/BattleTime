import type { DfuLink } from '@/dfu/link';
import { MockDfuTarget } from '@/dfu/mock-target';

import { MockPackages } from './mock-packages';
import type {
  ConnectionState,
  DiscoveredWatch,
  TransportListener,
  WatchMode,
  WatchTransport,
} from './transport';

const MOCK_WATCH = 'mock-pinetime';
const MOCK_BOOTLOADER = 'mock-dfutarg';

// A fake watch so the UI runs on web and in Expo Go, where the BLE native module is unavailable.
export class MockTransport implements WatchTransport {
  readonly kind = 'mock' as const;

  private listener: TransportListener = {};
  private state: ConnectionState = 'disconnected';
  private scanTimer: ReturnType<typeof setTimeout> | null = null;
  private packages = new MockPackages();
  private dfu: MockDfuTarget | null = null;
  private mode: WatchMode = 'application';

  async startScan(onFound: (watch: DiscoveredWatch) => void) {
    this.scanTimer = setTimeout(() => {
      onFound({ id: MOCK_WATCH, name: 'PineTime (mock)', rssi: -58 });
      // A second mock watch waiting in its bootloader, so that half of the
      // app can be worked on without holding a half-flashed watch.
      onFound({ id: MOCK_BOOTLOADER, name: 'DfuTarg (mock)', rssi: -61, bootloader: true });
    }, 600);
  }

  async stopScan() {
    if (this.scanTimer) {
      clearTimeout(this.scanTimer);
      this.scanTimer = null;
    }
  }

  async connect(id: string) {
    this.setState('connecting');
    await new Promise((resolve) => setTimeout(resolve, 500));
    this.mode = id === MOCK_BOOTLOADER ? 'bootloader' : 'application';
    this.setState('connected');
    this.listener.onMode?.(this.mode);
    if (this.mode === 'application') {
      this.listener.onLine?.('{"t":"info","msg":"mock watch connected"}');
    }
  }

  async disconnect() {
    this.setState('disconnected');
  }

  async write(text: string) {
    if (this.state !== 'connected') {
      throw new Error('Not connected');
    }
    if (this.mode === 'bootloader') {
      // A bootloader has no UART to write to.
      return;
    }

    // The mock watch answers package commands, so the Apps tab works without
    // hardware. Anything else is echoed so the console shows traffic.
    const replies = this.packages.handle(text);
    if (replies) {
      for (const line of replies) {
        this.listener.onLine?.(`${line}\r\n`);
      }
      return;
    }

    this.listener.onLine?.(`{"t":"info","msg":"mock received ${text.trim().length} bytes"}\r\n`);
  }

  setListener(listener: TransportListener) {
    this.listener = listener;
  }

  dfuLink(): DfuLink | null {
    if (this.state !== 'connected') {
      return null;
    }
    this.dfu ??= new MockDfuTarget();
    return this.dfu;
  }

  destroy() {
    void this.stopScan();
  }

  private setState(state: ConnectionState) {
    this.state = state;
    this.listener.onState?.(state);
  }
}
