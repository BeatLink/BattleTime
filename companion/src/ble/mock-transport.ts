import type { DfuLink } from '@/dfu/link';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';

import { MockGadgetbridge } from './mock-gadgetbridge';
import type {
  ConnectionState,
  DiscoveredWatch,
  TransportListener,
  WatchMode,
  WatchTransport,
} from './transport';

const MOCK_WATCH = 'AA:BB:CC:DD:EE:00';
const MOCK_BOOTLOADER = 'AA:BB:CC:DD:EE:11';

// A fake watch, so the window runs in a plain browser, where there is no Bluetooth bridge.
export class MockTransport implements WatchTransport {
  readonly kind = 'mock' as const;

  private listener: TransportListener = {};
  private state: ConnectionState = 'disconnected';
  private scanTimer: ReturnType<typeof setTimeout> | null = null;
  private gadgetbridge = new MockGadgetbridge();
  private dfu: MockLegacyDfuTarget | null = null;
  private mode: WatchMode = 'application';

  async startScan(onFound: (watch: DiscoveredWatch) => void) {
    this.scanTimer = setTimeout(() => {
      onFound({ id: MOCK_WATCH, name: 'BattleTime (mock)', rssi: -58 });
      // A second mock watch, still on wasp-os and waiting in its bootloader, for the move onto BattleTime.
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
    this.dfu =
      this.mode === 'bootloader'
        ? new MockLegacyDfuTarget(id, 'wasp-os')
        : new MockLegacyDfuTarget(id, 'battletime', true);
    if (this.mode === 'bootloader') {
      this.dfu.enterBootloader();
    } else {
      this.dfu.fileService.addSampleRecords(new Date());
    }
    this.setState('connected');
    this.listener.onMode?.(this.mode);
    if (this.mode === 'application') {
      this.listener.onLine?.('{"t":"info","msg":"mock watch connected"}\r\n');
    }
  }

  async disconnect() {
    this.setState('disconnected');
  }

  async writeBytes(data: Uint8Array) {
    await this.write(new TextDecoder().decode(data));
  }

  async write(text: string) {
    if (this.state !== 'connected') {
      throw new Error('Not connected');
    }
    if (this.mode === 'bootloader') {
      // A bootloader has no UART to write to.
      return;
    }

    // The mock watch answers the messages that expect a reply, so those screens work without
    // hardware. Anything else is echoed so the console shows traffic.
    const replies = this.gadgetbridge.handle(text);
    if (replies) {
      for (const line of replies) {
        this.listener.onLine?.(`${line}\r\n`);
      }
      return;
    }

    this.listener.onLine?.(`{"t":"info","msg":"mock received ${text.trim().length} bytes"}\r\n`);
  }

  // Delivers a line as if the watch had sent it, such as a remote button tap; ignored while disconnected.
  simulateWatchLine(line: string) {
    if (this.state === 'connected' && this.mode === 'application') {
      this.listener.onLine?.(`${line}\r\n`);
    }
  }

  setListener(listener: TransportListener) {
    this.listener = listener;
  }

  dfuLink(): DfuLink | null {
    if (this.state !== 'connected') {
      return null;
    }
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
