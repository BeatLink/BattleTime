// The command line tool's Bluetooth session: one connection to one watch,
// offering the same seams the app uses, so every command is the same code
// the screens run.

import type { WatchMode } from '@/ble/transport';
import { NUS_SERVICE } from '@/ble/uuids';
import { LEGACY_DFU_SERVICE } from '@/dfu/uuids';
import type { DfuLink, WriteMode } from '@/dfu/link';
import { decodeWatchLine, encodeForWatch, LineBuffer } from '@/protocol/gadgetbridge';
import { FileService } from '@/protocol/file-service';
import { PackagesClient } from '@/protocol/packages';
import { ReplyRouter, type RequestChannel } from '@/protocol/requests';

import { WatchBle, type ScannedDevice, type WatchBleEvent } from '../node/ble';

// How long a scan runs before reporting what it found.
export const DEFAULT_SCAN_MS = 6000;

export class WatchSession implements DfuLink {
  private readonly ble: WatchBle;
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

  constructor(private readonly verbose = false) {
    this.ble = new WatchBle(<K extends keyof WatchBleEvent>(name: K, payload: WatchBleEvent[K]) => {
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

  async connect(id: string): Promise<void> {
    await this.ble.connect(id, true);
    this.id = id;
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
      void this.ble.gattUnsubscribe(service, characteristic);
    };
  }

  async reconnect(deviceId: string): Promise<void> {
    await this.ble.reconnect(deviceId);
    this.id = deviceId;
  }
}
