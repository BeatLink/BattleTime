// A stand-in legacy DFU bootloader, the protocol the PineTime's own
// bootloader speaks, so that path is exercised without hardware too.

import { concat, readU32le, u32le } from './bytes';
import type { DfuLink, WriteMode } from './link';
import { Procedure } from './legacy';
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

export class MockLegacyDfuTarget implements DfuLink {
  readonly received: Received = { init: new Uint8Array(0), image: new Uint8Array(0) };

  private mode: 'application' | 'bootloader' = 'application';
  private notify: ((value: Uint8Array) => void) | null = null;
  private stage: Stage = 'idle';
  private expected = 0;
  private packets = 0;
  private activated = false;

  constructor(private id = 'AA:BB:CC:DD:EE:00') {}

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

  // The DFU service belongs to the bootloader; the application it starts
  // does not carry it.
  async hasCharacteristic(service: string, _characteristic: string): Promise<boolean> {
    return service === LEGACY_DFU_SERVICE && this.mode === 'bootloader';
  }

  async read(_service: string, characteristic: string): Promise<Uint8Array> {
    if (characteristic === LEGACY_VERSION && this.mode === 'bootloader') {
      return Uint8Array.from([0x08, 0x00]);
    }
    return new Uint8Array(0);
  }

  async subscribe(
    _service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    if (characteristic === LEGACY_CONTROL_POINT) {
      this.notify = onValue;
    }
    return () => {
      if (characteristic === LEGACY_CONTROL_POINT) {
        this.notify = null;
      }
    };
  }

  async reconnect(deviceId: string): Promise<void> {
    this.id = deviceId;
    this.mode = 'bootloader';
  }

  async write(
    _service: string,
    characteristic: string,
    data: Uint8Array,
    _mode: WriteMode,
  ): Promise<void> {
    if (characteristic === LEGACY_PACKET) {
      this.packet(data);
      return;
    }
    if (characteristic === LEGACY_CONTROL_POINT) {
      this.control(data);
    }
  }

  private respond(procedure: number) {
    this.notify?.(Uint8Array.from([Procedure.RESPONSE, procedure, SUCCESS]));
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
        this.notify?.(Uint8Array.from([Procedure.RESPONSE, data[0], 2]));
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
      this.notify?.(
        concat([Procedure.PACKET_RECEIPT_NOTIFICATION], u32le(this.received.image.length)),
      );
    }
    if (this.received.image.length >= this.expected) {
      this.stage = 'idle';
      this.respond(Procedure.RECEIVE_FIRMWARE_IMAGE);
    }
  }
}
