// The command line tool's Bluetooth session: one connection to one watch,
// offering the same seams the app uses, so every command is the same code
// the screens run. The link is the companion app's when it is running, and BlueZ's otherwise.

import type { WatchMode } from '@/ble/transport';
import { NUS_SERVICE } from '@/ble/uuids';
import { LEGACY_DFU_SERVICE } from '@/dfu/uuids';
import type { DfuLink, WriteMode } from '@/dfu/link';
import { decodeWatchLine, encodeForWatch, LineBuffer } from '@/protocol/gadgetbridge';
import { FileService } from '@/protocol/file-service';
import { PackagesClient } from '@/protocol/packages';
import { ReplyRouter, type RequestChannel } from '@/protocol/requests';

import { WatchBle, type ScannedDevice, type WatchBleEvent } from '../node/ble';
import { DaemonClient, DaemonLink, socketPath, type Emit } from './daemon';

// How long a scan runs before reporting what it found.
export const DEFAULT_SCAN_MS = 6000;

// What a session needs from its link to the watch: node/ble.js over BlueZ, or the app's link through its socket.
export type WatchLink = {
  startScan(): Promise<void>;
  stopScan(): Promise<void>;
  connect(id: string, uart?: boolean, name?: string): Promise<void>;
  reconnect(id: string): Promise<void>;
  write(text: string): Promise<void>;
  gattHas(service: string, characteristic: string): Promise<boolean>;
  gattRead(service: string, characteristic: string): Promise<number[]>;
  gattWrite(service: string, characteristic: string, data: number[], mode: WriteMode): Promise<void>;
  gattSubscribe(service: string, characteristic: string): Promise<void>;
  gattUnsubscribe(service: string, characteristic: string): Promise<void>;
  exclusive?<T>(work: () => Promise<T>): Promise<T>;
  hold?(held: boolean): Promise<void>;
  destroy(): Promise<void>;
};

export type LinkMaker = (emit: Emit) => WatchLink;

const throughBlueZ: LinkMaker = (emit) => new WatchBle(emit);

// The app's link when it answers on its socket, unless `direct` asks for BlueZ.
export async function openSession(verbose: boolean, direct: boolean): Promise<WatchSession> {
  const client = direct ? null : await DaemonClient.open(socketPath());
  if (!client) {
    return new WatchSession(verbose);
  }
  return new WatchSession(verbose, (emit) => new DaemonLink(client, emit), true);
}

export class WatchSession implements DfuLink {
  private readonly ble: WatchLink;
  readonly exclusive?: <T>(work: () => Promise<T>) => Promise<T>;
  private readonly lines = new LineBuffer();
  private fileService: Promise<FileService> | null = null;
  private packagesClient: Promise<PackagesClient> | null = null;
  private readonly router = new ReplyRouter((message) => this.send(encodeForWatch(message)));
  private readonly notifyListeners = new Map<string, (value: Uint8Array) => void>();
  private readonly found = new Map<string, ScannedDevice>();
  private onLine: ((line: string) => void) | null = null;
  private readonly disconnectListeners: (() => void)[] = [];
  private mode: WatchMode = 'application';
  private id = '';

  constructor(
    private readonly verbose = false,
    makeLink: LinkMaker = throughBlueZ,
    // True when the companion app carries the link.
    readonly throughApp = false,
  ) {
    this.ble = makeLink(<K extends keyof WatchBleEvent>(name: K, payload: WatchBleEvent[K]) => {
      if (name === 'device') {
        const watch = payload as ScannedDevice;
        this.found.set(watch.id, watch);
        return;
      }
      if (name === 'line') {
        for (const line of this.lines.push(payload as string)) {
          if (this.verbose) {
            process.stderr.write(`< ${line}\n`);
          }
          const message = decodeWatchLine(line);
          if (message && this.router.push(message)) {
            continue;
          }
          this.onLine?.(line);
        }
        return;
      }
      if (name === 'notify') {
        const { key, value } = payload as WatchBleEvent['notify'];
        this.notifyListeners.get(key)?.(Uint8Array.from(value));
        return;
      }
      if (name === 'mode') {
        this.mode = payload as WatchMode;
        return;
      }
      if (name === 'state' && payload === 'disconnected') {
        for (const listener of this.disconnectListeners.splice(0)) listener();
        return;
      }
      if (name === 'error' && this.verbose) {
        process.stderr.write(`! ${String(payload)}\n`);
      }
    });
    const link = this.ble;
    if (link.exclusive) {
      this.exclusive = (work) => link.exclusive!(work);
    }
  }

  get deviceId() {
    return this.id;
  }

  // Look for watches for a while and report what turned up. A watch advertises the UART service or
  // the DFU service; when nothing does either, everything found is reported, because BlueZ may not
  // have read the services yet.
  async scan(ms = DEFAULT_SCAN_MS): Promise<ScannedDevice[]> {
    this.found.clear();
    await this.ble.startScan();
    await new Promise((resolve) => setTimeout(resolve, ms));
    await this.ble.stopScan();

    const all = [...this.found.values()];
    const watches = all.filter(
      (device) => device.uuids.includes(NUS_SERVICE) || device.uuids.includes(LEGACY_DFU_SERVICE),
    );
    return watches.length > 0 ? watches : all;
  }

  async connect(id: string, name = ''): Promise<void> {
    await this.ble.connect(id, true, name);
    this.id = id;
  }

  // Runs a firmware update with the app held back from reconnecting the watch under it.
  async holding<T>(work: () => Promise<T>): Promise<T> {
    await this.ble.hold?.(true);
    try {
      return await work();
    } finally {
      await this.ble.hold?.(false).catch(() => undefined);
    }
  }

  // Whether the watch answered as a bootloader rather than as firmware.
  get bootloader(): boolean {
    return this.mode === 'bootloader';
  }

  async close(): Promise<void> {
    await this.ble.destroy().catch(() => undefined);
  }

  // Raw text over the UART channel, which the Gadgetbridge messages are built on.
  async send(text: string): Promise<void> {
    if (this.verbose) {
      process.stderr.write(`> ${text.trimEnd()}\n`);
    }
    await this.ble.write(text);
  }

  // Resolves when the link to the watch drops, or is closed.
  whenDisconnected(): Promise<void> {
    return new Promise((resolve) => this.disconnectListeners.push(resolve));
  }

  // Lines the watch sends that are not replies to a request.
  listen(onLine: (line: string) => void) {
    this.onLine = onLine;
  }

  // BattleTime's file service, opened on first use.
  files(): Promise<FileService> {
    this.fileService ??= FileService.open(this);
    return this.fileService;
  }

  // The packages characteristic, opened on first use.
  packages(): Promise<PackagesClient> {
    this.packagesClient ??= PackagesClient.open(this);
    return this.packagesClient;
  }

  // The Gadgetbridge messages the watch answers: settings, alarms and steps.
  get requests(): RequestChannel {
    return this.router;
  }

  // --- DfuLink, so a firmware update runs the same controllers the app does.

  async hasCharacteristic(service: string, characteristic: string): Promise<boolean> {
    return this.ble.gattHas(service, characteristic);
  }

  async read(service: string, characteristic: string): Promise<Uint8Array> {
    return Uint8Array.from(await this.ble.gattRead(service, characteristic));
  }

  async write(service: string, characteristic: string, data: Uint8Array, mode: WriteMode) {
    await this.ble.gattWrite(service, characteristic, Array.from(data), mode);
  }

  async subscribe(
    service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    const key = `${service}/${characteristic}`;
    this.notifyListeners.set(key, onValue);
    await this.ble.gattSubscribe(service, characteristic);
    return () => {
      this.notifyListeners.delete(key);
      void this.ble.gattUnsubscribe(service, characteristic).catch(() => undefined);
    };
  }

  async reconnect(deviceId: string): Promise<void> {
    await this.ble.reconnect(deviceId);
    this.id = deviceId;
  }
}
