import type { DfuLink, WriteMode } from '@/dfu/link';
import { daemonBridge } from '@/services/daemon';

import type {
  ConnectionState,
  DiscoveredWatch,
  TransportListener,
  WatchMode,
  WatchTransport,
} from './transport';

// The Bluetooth part of window.battletime, which tauri/src/bridge.js gives the window.
export type BluetoothBridge = {
  startScan(): Promise<BridgeResult>;
  stopScan(): Promise<BridgeResult>;
  connect(id: string): Promise<BridgeResult>;
  disconnect(): Promise<BridgeResult>;
  write(text: string): Promise<BridgeResult>;
  writeBytes(data: number[]): Promise<BridgeResult>;
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
  // The core shares the file service with its sync, so the window takes a turn for each operation.
  filesBegin(): Promise<BridgeResult>;
  filesEnd(lease: unknown): Promise<BridgeResult>;
  on(
    name: 'device' | 'line' | 'state' | 'error' | 'notify' | 'mode',
    listener: (payload: never) => void,
  ): () => void;
};

// One characteristic's notification, as the core forwards it.
type Notification = { key: string; value: number[] };

type BridgeResult = { ok: boolean; error?: string; value?: unknown };

export function bluetoothBridge(): BluetoothBridge | null {
  const bridge = (globalThis as { battletime?: BluetoothBridge }).battletime;
  return bridge ?? null;
}

function check(result: BridgeResult) {
  if (!result.ok) {
    throw new Error(result.error ?? 'The Bluetooth bridge failed.');
  }
}

// Talks to a real watch through the app's Rust core, which owns the Bluetooth connection.
export class TauriTransport implements WatchTransport {
  readonly kind = 'tauri' as const;

  private listener: TransportListener = {};
  private onFound: ((watch: DiscoveredWatch) => void) | null = null;
  private unsubscribe: (() => void)[] = [];
  private deviceId: string | null = null;
  private readonly notifyListeners = new Map<string, Set<(value: Uint8Array) => void>>();

  constructor(private readonly bridge: BluetoothBridge) {
    this.unsubscribe.push(
      bridge.on('device', ((watch: DiscoveredWatch) => this.onFound?.(watch)) as never),
      bridge.on('line', ((line: string) => this.listener.onLine?.(line)) as never),
      bridge.on('state', ((state: ConnectionState) => this.listener.onState?.(state)) as never),
      bridge.on('mode', ((mode: WatchMode) => this.listener.onMode?.(mode)) as never),
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

  async writeBytes(data: Uint8Array) {
    check(await this.bridge.writeBytes(Array.from(data)));
  }

  setListener(listener: TransportListener) {
    this.listener = listener;
  }

  dfuLink(): DfuLink | null {
    return this.deviceId ? new TauriDfuLink(this, this.bridge, this.deviceId) : null;
  }

  // Used by the DFU link, which needs the notifications the core forwards for characteristics outside the UART service.
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
    // The daemon keeps the watch after the window goes.
    if (!daemonBridge()) {
      void this.bridge.disconnect();
    }
  }
}

// Raw GATT through the same bridge, which carries bytes as arrays.
class TauriDfuLink implements DfuLink {
  constructor(
    private readonly transport: TauriTransport,
    private readonly bridge: BluetoothBridge,
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

  async exclusive<T>(work: () => Promise<T>): Promise<T> {
    const begun = await this.bridge.filesBegin();
    check(begun);
    try {
      return await work();
    } finally {
      await this.bridge.filesEnd(begun.value);
    }
  }

  async hold(held: boolean): Promise<void> {
    await daemonBridge()?.hold(held);
  }

  async reconnect(deviceId: string): Promise<void> {
    check(await this.bridge.reconnect(deviceId));
    this.id = deviceId;
    this.transport.setDeviceId(deviceId);
  }
}
