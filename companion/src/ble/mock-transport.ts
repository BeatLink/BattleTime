import type { ConnectionState, DiscoveredWatch, TransportListener, WatchTransport } from './transport';

// A fake watch so the UI runs on web and in Expo Go, where the BLE native module is unavailable.
export class MockTransport implements WatchTransport {
  private listener: TransportListener = {};
  private state: ConnectionState = 'disconnected';
  private scanTimer: ReturnType<typeof setTimeout> | null = null;

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
    // Echo the message back so the console shows traffic in both directions.
    this.listener.onLine?.(`{"t":"info","msg":"mock received ${text.trim().length} bytes"}`);
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
