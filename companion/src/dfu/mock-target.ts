// A stand-in secure DFU bootloader, so the firmware screen and its tests can
// run a whole update with no watch attached.

import { concat, crc32, nextAddress, readU32le, u32le } from './bytes';
import type { DfuLink, WriteMode } from './link';
import { Procedure } from './secure';
import {
  SECURE_BUTTONLESS,
  SECURE_CONTROL_POINT,
  SECURE_DFU_SERVICE,
  SECURE_PACKET,
} from './uuids';

const SUCCESS = 0x01;
const OBJECT_COMMAND = 0x01;

// The buffer a real bootloader offers for one data object.
const MAX_OBJECT = 4096;
const RECEIPT_INTERVAL = 10;

type Received = { init: Uint8Array; image: Uint8Array };

export class MockDfuTarget implements DfuLink {
  // What the update left behind, for a test to check.
  readonly received: Received = { init: new Uint8Array(0), image: new Uint8Array(0) };

  private mode: 'application' | 'bootloader' = 'application';
  private notify: ((value: Uint8Array) => void) | null = null;
  private object: { type: number; size: number } | null = null;
  private objectStart = 0;
  private packets = 0;

  constructor(private id = 'AA:BB:CC:DD:EE:00') {}

  get deviceId() {
    return this.id;
  }

  // Reboot into the bootloader, as a reset over the UART line would.
  enterBootloader() {
    this.mode = 'bootloader';
    this.id = nextAddress(this.id);
  }

  async hasCharacteristic(service: string, characteristic: string): Promise<boolean> {
    if (service !== SECURE_DFU_SERVICE) {
      return false;
    }
    if (this.mode === 'application') {
      return characteristic === SECURE_BUTTONLESS;
    }
    return characteristic === SECURE_CONTROL_POINT || characteristic === SECURE_PACKET;
  }

  async read(): Promise<Uint8Array> {
    return new Uint8Array(0);
  }

  async subscribe(
    _service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    if (characteristic === SECURE_CONTROL_POINT) {
      this.notify = onValue;
    }
    return () => {
      if (characteristic === SECURE_CONTROL_POINT) {
        this.notify = null;
      }
    };
  }

  async reconnect(deviceId: string): Promise<void> {
    this.id = deviceId;
  }

  async write(
    _service: string,
    characteristic: string,
    data: Uint8Array,
    _mode: WriteMode,
  ): Promise<void> {
    if (characteristic === SECURE_BUTTONLESS) {
      this.enterBootloader();
      return;
    }
    if (characteristic === SECURE_PACKET) {
      this.data(data);
      return;
    }
    if (characteristic === SECURE_CONTROL_POINT) {
      this.control(data);
    }
  }

  private respond(procedure: number, extra: Uint8Array = new Uint8Array(0)) {
    this.notify?.(concat([Procedure.RESPONSE, procedure, SUCCESS], extra));
  }

  // The offset and CRC of whatever object is being written, which is what
  // both the checksum command and the packet receipts report.
  private progress(): Uint8Array {
    const target = this.object?.type === OBJECT_COMMAND ? this.received.init : this.received.image;
    return concat(u32le(target.length), u32le(crc32(target)));
  }

  private control(data: Uint8Array) {
    switch (data[0]) {
      case Procedure.SET_PRN:
        this.respond(Procedure.SET_PRN);
        return;
      case Procedure.SELECT: {
        const type = data[1];
        const target = type === OBJECT_COMMAND ? this.received.init : this.received.image;
        this.object = { type, size: 0 };
        this.respond(
          Procedure.SELECT,
          concat(u32le(MAX_OBJECT), u32le(target.length), u32le(crc32(target))),
        );
        return;
      }
      case Procedure.CREATE: {
        const type = data[1];
        this.object = { type, size: readU32le(data, 2) };
        this.packets = 0;
        const target = type === OBJECT_COMMAND ? this.received.init : this.received.image;
        // A fresh command object replaces the init packet; a data object is
        // appended to what has already been accepted.
        if (type === OBJECT_COMMAND) {
          this.received.init = new Uint8Array(0);
          this.objectStart = 0;
        } else {
          this.objectStart = target.length;
        }
        this.respond(Procedure.CREATE);
        return;
      }
      case Procedure.CALC_CHECKSUM:
        this.respond(Procedure.CALC_CHECKSUM, this.progress());
        return;
      case Procedure.EXECUTE:
        this.object = null;
        this.respond(Procedure.EXECUTE);
        return;
      default:
        this.notify?.(Uint8Array.from([Procedure.RESPONSE, data[0], 0x02]));
    }
  }

  private data(chunk: Uint8Array) {
    if (!this.object) {
      return;
    }
    const key = this.object.type === OBJECT_COMMAND ? 'init' : 'image';
    const written = this.received[key].length - this.objectStart;
    // Ignore anything past the object the bootloader was told to expect.
    const room = Math.max(0, this.object.size - written);
    this.received[key] = concat(this.received[key], chunk.subarray(0, room));

    this.packets += 1;
    if (this.packets % RECEIPT_INTERVAL === 0) {
      this.respond(Procedure.CALC_CHECKSUM, this.progress());
    }
  }
}
