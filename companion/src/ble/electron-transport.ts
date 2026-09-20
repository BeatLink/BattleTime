import type {
  ConnectionState,
  DiscoveredWatch,
  TransportListener,
  WatchTransport,
} from './transport';

// What the Electron preload exposes on the window.
export type DesktopBridge = {
  startScan(): Promise<BridgeResult>;
  stopScan(): Promise<BridgeResult>;
  connect(id: string): Promise<BridgeResult>;
  disconnect(): Promise<BridgeResult>;
  write(text: string): Promise<BridgeResult>;
  on(name: 'device' | 'line' | 'state' | 'error', listener: (payload: never) => void): () => void;
};

type BridgeResult = { ok: boolean; error?: string; value?: unknown };

export function desktopBridge(): DesktopBridge | null {
  const bridge = (globalThis as { neotime?: DesktopBridge }).neotime;
  return bridge ?? null;
}

function check(result: BridgeResult) {
  if (!result.ok) {
    throw new Error(result.error ?? 'The desktop Bluetooth bridge failed.');
  }
}

// Talks to a real watch through the Electron main process, which owns the
// Bluetooth connection.
export class ElectronTransport implements WatchTransport {
  readonly kind = 'electron' as const;

  private listener: TransportListener = {};
  private onFound: ((watch: DiscoveredWatch) => void) | null = null;
  private unsubscribe: (() => void)[] = [];

  constructor(private readonly bridge: DesktopBridge) {
    this.unsubscribe.push(
      bridge.on('device', ((watch: DiscoveredWatch) => this.onFound?.(watch)) as never),
      bridge.on('line', ((line: string) => this.listener.onLine?.(line)) as never),
      bridge.on('state', ((state: ConnectionState) => this.listener.onState?.(state)) as never),
      bridge.on('error', ((message: string) =>
        this.listener.onError?.(new Error(message))) as never),
    );
  }

  async startScan(onFound: (watch: DiscoveredWatch) => void) {
    this.onFound = onFound;
    check(await this.bridge.startScan());
  }

  async stopScan() {
    this.onFound = null;
    check(await this.bridge.stopScan());
  }

  async connect(id: string) {
    check(await this.bridge.connect(id));
  }

  async disconnect() {
    check(await this.bridge.disconnect());
  }

  async write(text: string) {
    check(await this.bridge.write(text));
  }

  setListener(listener: TransportListener) {
    this.listener = listener;
  }

  destroy() {
    for (const off of this.unsubscribe) {
      off();
    }
    this.unsubscribe = [];
    void this.bridge.disconnect();
  }
}
