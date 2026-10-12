// A stand-in watch that takes updates over legacy DFU: NeoTime, which serves DFU in its firmware,
// or wasp-os, which serves it only from its bootloader.

import { concat, readU32le, u32le } from './bytes';
import type { DfuLink, WriteMode } from './link';
import { Procedure } from './legacy';
import { NUS_SERVICE } from '@/ble/uuids';
import { MockFileService } from '@/protocol/mock-file-service';
import { MockPackagesService } from '@/protocol/mock-packages-service';
import {
  CURRENT_TIME,
  CURRENT_TIME_SERVICE,
  FILE_TRANSFER,
  NEOTIME_DIAGNOSTICS,
  NEOTIME_PACKAGES,
  NEOTIME_SERVICE,
} from '@/protocol/neotime';

import {
  LEGACY_CONTROL_POINT,
  LEGACY_DFU_SERVICE,
  LEGACY_PACKET,
  LEGACY_VERSION,
} from './uuids';

const SUCCESS = 1;
const RECEIPT_INTERVAL = 5;

type Stage = 'idle' | 'size' | 'init' | 'image';

type Received = { init: Uint8Array; image: Uint8Array };

export type MockFirmware = 'neotime' | 'wasp-os';

export class MockLegacyDfuTarget implements DfuLink {
  readonly received: Received = { init: new Uint8Array(0), image: new Uint8Array(0) };

  private mode: 'application' | 'bootloader' = 'application';
  // Who is listening on each characteristic.
  private readonly listeners = new Map<string, Set<(value: Uint8Array) => void>>();
  // Part of a line written to the shared control characteristic.
  private pendingControl = '';
  private stage: Stage = 'idle';
  private expected = 0;
  private packets = 0;
  private activated = false;
  // Every Current Time value written, which is what keeps a NeoTime update.
  readonly timeWrites: Uint8Array[] = [];

  constructor(
    private id = 'AA:BB:CC:DD:EE:00',
    readonly firmware: MockFirmware = 'neotime',
    // Whether the firmware also serves the UART message channel and the file service.
    private readonly uart = false,
  ) {
    this.fileService = new MockFileService((value) => this.emit(FILE_TRANSFER, value));
    this.packagesService = new MockPackagesService(this.fileService.files, (value) => this.emit(NEOTIME_PACKAGES, value));
  }

  // NeoTime's files, which tests read back.
  readonly fileService: MockFileService;
  // The packages commands, which share the control characteristic and the files.
  readonly packagesService: MockPackagesService;

  private emit(characteristic: string, value: Uint8Array) {
    for (const listener of this.listeners.get(characteristic) ?? []) {
      listener(value);
    }
  }

  get deviceId() {
    return this.id;
  }

  // Whether the update reached the command that reboots into the new image.
  get finished() {
    return this.activated;
  }

  enterBootloader() {
    this.mode = 'bootloader';
  }

  private get neotime() {
    return this.firmware === 'neotime';
  }

  // NeoTime's firmware serves DFU, time and diagnostics; wasp-os serves DFU only from its bootloader.
  async hasCharacteristic(service: string, characteristic: string): Promise<boolean> {
    if (service === LEGACY_DFU_SERVICE) {
      return this.neotime || this.mode === 'bootloader';
    }
    if (characteristic === FILE_TRANSFER) {
      return this.neotime && this.uart && this.mode === 'application';
    }
    if (service === CURRENT_TIME_SERVICE || service === NEOTIME_SERVICE) {
      return this.neotime && this.mode === 'application';
    }
    if (service === NUS_SERVICE) {
      return this.uart && this.mode === 'application';
    }
    return false;
  }

  async read(_service: string, characteristic: string): Promise<Uint8Array> {
    if (characteristic === LEGACY_VERSION && (this.neotime || this.mode === 'bootloader')) {
      return Uint8Array.from([0x08, 0x00]);
    }
    if (characteristic === NEOTIME_DIAGNOSTICS && this.neotime) {
      return concat(u32le(1234), u32le(1), u32le(1), u32le(1), u32le(56), u32le(7), u32le(3), u32le(1));
    }
    return new Uint8Array(0);
  }

  async subscribe(
    _service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    const listeners = this.listeners.get(characteristic) ?? new Set();
    listeners.add(onValue);
    this.listeners.set(characteristic, listeners);
    return () => {
      listeners.delete(onValue);
    };
  }

  // A watch that activated an update comes back on its firmware; wasp-os otherwise comes back in its bootloader.
  async reconnect(deviceId: string): Promise<void> {
    this.id = deviceId;
    this.mode = this.activated || this.neotime ? 'application' : 'bootloader';
  }

  async write(
    _service: string,
    characteristic: string,
    data: Uint8Array,
    _mode: WriteMode,
  ): Promise<void> {
    if (characteristic === CURRENT_TIME) {
      this.timeWrites.push(data);
      return;
    }
    if (characteristic === NEOTIME_PACKAGES) {
      this.routeControl(data);
      return;
    }
    if (characteristic === FILE_TRANSFER) {
      this.fileService.transfer(data);
      return;
    }
    if (characteristic === LEGACY_PACKET) {
      this.packet(data);
      return;
    }
    if (characteristic === LEGACY_CONTROL_POINT) {
      this.control(data);
    }
  }

  // Packages commands, a line at a time.
  private routeControl(data: Uint8Array) {
    this.pendingControl += new TextDecoder().decode(data);
    let newline = this.pendingControl.indexOf('\n');
    while (newline >= 0) {
      const line = this.pendingControl.slice(0, newline);
      this.pendingControl = this.pendingControl.slice(newline + 1);
      newline = this.pendingControl.indexOf('\n');
      this.packagesService.handle(JSON.parse(line) as { c: string });
    }
  }

  private respond(procedure: number) {
    this.emit(LEGACY_CONTROL_POINT, Uint8Array.from([Procedure.RESPONSE, procedure, SUCCESS]));
  }

  private control(data: Uint8Array) {
    switch (data[0]) {
      case Procedure.START_DFU:
        this.stage = 'size';
        return;
      case Procedure.INITIALIZE_DFU:
        if (data[1] === 0x00) {
          this.stage = 'init';
          this.received.init = new Uint8Array(0);
        } else {
          this.stage = 'idle';
          this.respond(Procedure.INITIALIZE_DFU);
        }
        return;
      case Procedure.PRN_REQUEST:
        return;
      case Procedure.RECEIVE_FIRMWARE_IMAGE:
        this.stage = 'image';
        this.packets = 0;
        return;
      case Procedure.VALIDATE_FIRMWARE:
        this.respond(Procedure.VALIDATE_FIRMWARE);
        return;
      case Procedure.ACTIVATE_IMAGE_AND_RESET:
        this.activated = true;
        return;
      default:
        this.emit(LEGACY_CONTROL_POINT, Uint8Array.from([Procedure.RESPONSE, data[0], 2]));
    }
  }

  private packet(chunk: Uint8Array) {
    if (this.stage === 'size') {
      // Softdevice, bootloader and application sizes, in that order.
      this.expected = readU32le(chunk, 8);
      this.stage = 'idle';
      this.respond(Procedure.START_DFU);
      return;
    }
    if (this.stage === 'init') {
      this.received.init = concat(this.received.init, chunk);
      return;
    }
    if (this.stage !== 'image') {
      return;
    }

    this.received.image = concat(this.received.image, chunk);
    this.packets += 1;
    if (this.packets % RECEIPT_INTERVAL === 0) {
      this.emit(LEGACY_CONTROL_POINT, 
        concat([Procedure.PACKET_RECEIPT_NOTIFICATION], u32le(this.received.image.length)),
      );
    }
    if (this.received.image.length >= this.expected) {
      this.stage = 'idle';
      this.respond(Procedure.RECEIVE_FIRMWARE_IMAGE);
    }
  }
}
