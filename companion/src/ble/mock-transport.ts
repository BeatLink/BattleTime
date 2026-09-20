import { MockPackages } from './mock-packages';
import type { ConnectionState, DiscoveredWatch, TransportListener, WatchTransport } from './transport';

// A fake watch so the UI runs on web and in Expo Go, where the BLE native module is unavailable.
export class MockTransport implements WatchTransport {
  private listener: TransportListener = {};
  private state: ConnectionState = 'disconnected';
  private scanTimer: ReturnType<typeof setTimeout> | null = null;
  private packages = new MockPackages();

  async startScan(onFound: (watch: DiscoveredWatch) => void) {
    this.scanTimer = setTimeout(() => {
      onFound({ id: 'mock-pinetime', name: 'PineTime (mock)', rssi: -58 });
    }, 600);
  }

  async stopScan() {
    if (this.scanTimer) {
      clearTimeout(this.scanTimer);
      this.scanTimer = null;
    }
  }

  async connect(_id: string) {
    this.setState('connecting');
    await new Promise((resolve) => setTimeout(resolve, 500));
    this.setState('connected');
    this.listener.onLine?.('{"t":"info","msg":"mock watch connected"}');
  }

  async disconnect() {
    this.setState('disconnected');
  }

  async write(text: string) {
    if (this.state !== 'connected') {
      throw new Error('Not connected');
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

  destroy() {
    void this.stopScan();
  }

  private setState(state: ConnectionState) {
    this.state = state;
    this.listener.onState?.(state);
  }
}
