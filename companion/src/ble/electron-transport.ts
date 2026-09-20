import type { DfuLink, WriteMode } from '@/dfu/link';

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
  gattHas(service: string, characteristic: string): Promise<BridgeResult>;
  gattRead(service: string, characteristic: string): Promise<BridgeResult>;
  gattWrite(
    service: string,
    characteristic: string,
    data: number[],
    mode: WriteMode,
  ): Promise<BridgeResult>;
  gattSubscribe(service: string, characteristic: string): Promise<BridgeResult>;
  gattUnsubscribe(service: string, characteristic: string): Promise<BridgeResult>;
  reconnect(id: string): Promise<BridgeResult>;
  on(
    name: 'device' | 'line' | 'state' | 'error' | 'notify',
    listener: (payload: never) => void,
  ): () => void;
};

// One characteristic's notification, as the main process forwards it.
type Notification = { key: string; value: number[] };

type BridgeResult = { ok: boolean; error?: string; value?: unknown };

export function desktopBridge(): DesktopBridge | null {
  const bridge = (globalThis as { waspos?: DesktopBridge }).waspos;
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
  private deviceId: string | null = null;
  private readonly notifyListeners = new Map<string, Set<(value: Uint8Array) => void>>();

  constructor(private readonly bridge: DesktopBridge) {
    this.unsubscribe.push(
      bridge.on('device', ((watch: DiscoveredWatch) => this.onFound?.(watch)) as never),
      bridge.on('line', ((line: string) => this.listener.onLine?.(line)) as never),
      bridge.on('state', ((state: ConnectionState) => this.listener.onState?.(state)) as never),
      bridge.on('error', ((message: string) =>
        this.listener.onError?.(new Error(message))) as never),
      bridge.on('notify', ((notification: Notification) => {
        const value = Uint8Array.from(notification.value);
        for (const listener of this.notifyListeners.get(notification.key) ?? []) {
          listener(value);
        }
      }) as never),
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
    this.deviceId = id;
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

  dfuLink(): DfuLink | null {
    return this.deviceId ? new ElectronDfuLink(this, this.bridge, this.deviceId) : null;
  }

  // Used by the DFU link, which needs the notifications the main process
  // forwards for characteristics outside the UART service.
  listen(key: string, onValue: (value: Uint8Array) => void): () => void {
    const listeners = this.notifyListeners.get(key) ?? new Set();
    listeners.add(onValue);
    this.notifyListeners.set(key, listeners);
    return () => listeners.delete(onValue);
  }

  setDeviceId(id: string) {
    this.deviceId = id;
  }

  destroy() {
    for (const off of this.unsubscribe) {
      off();
    }
    this.unsubscribe = [];
    void this.bridge.disconnect();
  }
}

// Raw GATT through the same preload bridge, which carries bytes as arrays.
class ElectronDfuLink implements DfuLink {
  constructor(
    private readonly transport: ElectronTransport,
    private readonly bridge: DesktopBridge,
    private id: string,
  ) {}

  get deviceId() {
    return this.id;
  }

  async hasCharacteristic(service: string, characteristic: string): Promise<boolean> {
    const result = await this.bridge.gattHas(service, characteristic);
    return result.ok && result.value === true;
  }

  async read(service: string, characteristic: string): Promise<Uint8Array> {
    const result = await this.bridge.gattRead(service, characteristic);
    check(result);
    return Uint8Array.from((result.value as number[]) ?? []);
  }

  async write(service: string, characteristic: string, data: Uint8Array, mode: WriteMode) {
    check(await this.bridge.gattWrite(service, characteristic, Array.from(data), mode));
  }

  async subscribe(
    service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    const off = this.transport.listen(`${service}/${characteristic}`, onValue);
    check(await this.bridge.gattSubscribe(service, characteristic));
    return () => {
      off();
      void this.bridge.gattUnsubscribe(service, characteristic);
    };
  }

  async reconnect(deviceId: string): Promise<void> {
    check(await this.bridge.reconnect(deviceId));
    this.id = deviceId;
    this.transport.setDeviceId(deviceId);
  }
}
