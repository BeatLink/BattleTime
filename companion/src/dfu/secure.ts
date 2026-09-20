// Nordic secure DFU, the protocol nRF5 SDK 12 and later bootloaders speak.
// Ported from wasp-os's tools/ota-dfu.

import { concat, crc32, nextAddress, readU32le, u16le, u32le } from './bytes';
import { DfuError, NotifyQueue, sleep, type DfuLink } from './link';
import type { DfuProgress } from './legacy';
import type { FirmwareImage } from './package';
import {
  DFU_PAYLOAD_SIZE,
  SECURE_BUTTONLESS,
  SECURE_CONTROL_POINT,
  SECURE_DFU_SERVICE,
  SECURE_PACKET,
} from './uuids';

export const Procedure = {
  CREATE: 0x01,
  SET_PRN: 0x02,
  CALC_CHECKSUM: 0x03,
  EXECUTE: 0x04,
  SELECT: 0x06,
  RESPONSE: 0x60,
} as const;

// Which object a CREATE or SELECT command is about.
const OBJECT_COMMAND = 0x01;
const OBJECT_DATA = 0x02;

const SUCCESS = 0x01;

const PROCEDURE_NAMES: Record<number, string> = {
  0x01: 'create object',
  0x02: 'set notification interval',
  0x03: 'checksum',
  0x04: 'execute',
  0x06: 'select object',
};

const RESULT_NAMES: Record<number, string> = {
  0x00: 'invalid code',
  0x01: 'success',
  0x02: 'opcode not supported',
  0x03: 'invalid parameter',
  0x04: 'insufficient resources',
  0x05: 'invalid object',
  0x07: 'unsupported type',
  0x08: 'operation not permitted',
  0x0a: 'operation failed',
};

const RECEIPT_INTERVAL = 10;

// How many times to re-send an object whose checksum came back wrong before
// giving up, rather than looping forever on a bad link.
const MAX_ATTEMPTS = 3;

export type SecureResponse = {
  procedure: number;
  result: number;
  // CALC_CHECKSUM and the packet receipts that share its shape.
  offset?: number;
  crc?: number;
  // SELECT also reports how large an object the bootloader will accept.
  maxSize?: number;
};

export function parseResponse(value: Uint8Array): SecureResponse {
  if (value.length < 3 || value[0] !== Procedure.RESPONSE) {
    throw new DfuError('The bootloader sent a notification that is not a DFU response.');
  }
  const response: SecureResponse = { procedure: value[1], result: value[2] };
  if (response.result !== SUCCESS) {
    return response;
  }
  if (response.procedure === Procedure.CALC_CHECKSUM && value.length >= 11) {
    response.offset = readU32le(value, 3);
    response.crc = readU32le(value, 7);
  } else if (response.procedure === Procedure.SELECT && value.length >= 15) {
    response.maxSize = readU32le(value, 3);
    response.offset = readU32le(value, 7);
    response.crc = readU32le(value, 11);
  }
  return response;
}

export class SecureDfuController {
  private readonly notifications = new NotifyQueue();
  private unsubscribe: (() => void) | null = null;

  constructor(private readonly link: DfuLink) {}

  // Only the application carries the buttonless characteristic, so its
  // absence means the bootloader is already running.
  static async inDfuMode(link: DfuLink): Promise<boolean> {
    return !(await link.hasCharacteristic(SECURE_DFU_SERVICE, SECURE_BUTTONLESS));
  }

  // Ask the application to reset into its bootloader, which comes back one
  // address higher than the watch.
  static async switchToDfuMode(link: DfuLink): Promise<void> {
    const bootloader = nextAddress(link.deviceId);
    const stop = await link.subscribe(SECURE_DFU_SERVICE, SECURE_BUTTONLESS, () => undefined);
    try {
      await link.write(
        SECURE_DFU_SERVICE,
        SECURE_BUTTONLESS,
        Uint8Array.from([0x01]),
        'request',
      ).catch(() => undefined);
    } finally {
      stop();
    }
    // The watch drops the link as it reboots.
    await sleep(1000);
    await link.reconnect(bootloader);
  }

  async run(image: FirmwareImage, onProgress?: (progress: DfuProgress) => void): Promise<void> {
    const total = image.image.length;
    const report = (phase: DfuProgress['phase'], sent: number) =>
      onProgress?.({ part: image.part, phase, sent, total });

    report('preparing', 0);
    this.unsubscribe = await this.link.subscribe(
      SECURE_DFU_SERVICE,
      SECURE_CONTROL_POINT,
      (value) => this.notifications.push(value),
    );

    try {
      await this.command(Procedure.SET_PRN, u16le(RECEIPT_INTERVAL));
      await this.expect(Procedure.SET_PRN);
      await this.sendInit(image.init);
      await this.sendImage(image.image, report);
      report('done', total);
    } finally {
      this.unsubscribe?.();
      this.unsubscribe = null;
    }
  }

  // The init packet is one command object, small enough to go in one piece.
  private async sendInit(init: Uint8Array): Promise<void> {
    await this.command(Procedure.SELECT, [OBJECT_COMMAND]);
    const selected = await this.expect(Procedure.SELECT);

    // An interrupted run may have left the whole init packet in place already.
    if (selected.offset !== init.length || selected.crc !== crc32(init)) {
      await this.command(Procedure.CREATE, concat([OBJECT_COMMAND], u32le(init.length)));
      await this.expect(Procedure.CREATE);
      await this.stream(init);
      await this.command(Procedure.CALC_CHECKSUM);
      const checksum = await this.expect(Procedure.CALC_CHECKSUM);
      if (checksum.crc !== crc32(init)) {
        throw new DfuError('The bootloader received a corrupted init packet.');
      }
    }

    await this.command(Procedure.EXECUTE);
    await this.expect(Procedure.EXECUTE, 120000);
  }

  // The image is split into objects no larger than the bootloader's buffer,
  // each created, streamed, checked and executed in turn.
  private async sendImage(
    image: Uint8Array,
    report: (phase: DfuProgress['phase'], sent: number) => void,
  ): Promise<void> {
    await this.command(Procedure.SELECT, [OBJECT_DATA]);
    const selected = await this.expect(Procedure.SELECT);
    const maxSize = selected.maxSize && selected.maxSize > 0 ? selected.maxSize : 4096;

    // Resume on an object boundary if a previous run got part way.
    let offset = Math.floor((selected.offset ?? 0) / maxSize) * maxSize;
    if (offset > 0 && selected.crc !== crc32(image.subarray(0, selected.offset ?? 0))) {
      offset = 0;
    }
    report('sending', offset);

    while (offset < image.length) {
      const end = Math.min(offset + maxSize, image.length);
      await this.sendObject(image, offset, end, report);
      offset = end;
    }
    report('sending', image.length);
  }

  private async sendObject(
    image: Uint8Array,
    start: number,
    end: number,
    report: (phase: DfuProgress['phase'], sent: number) => void,
  ): Promise<void> {
    for (let attempt = 1; ; attempt++) {
      await this.command(Procedure.CREATE, concat([OBJECT_DATA], u32le(end - start)));
      await this.expect(Procedure.CREATE);

      let segments = 0;
      let corrupt = false;
      for (let at = start; at < end && !corrupt; at += DFU_PAYLOAD_SIZE) {
        const stop = Math.min(at + DFU_PAYLOAD_SIZE, end);
        await this.packet(image.subarray(at, stop));
        segments += 1;
        if (segments % RECEIPT_INTERVAL === 0) {
          const receipt = await this.expect(Procedure.CALC_CHECKSUM);
          if (receipt.crc !== crc32(image.subarray(0, receipt.offset ?? 0))) {
            corrupt = true;
            break;
          }
          report('sending', receipt.offset ?? at);
        }
      }

      if (!corrupt) {
        await this.command(Procedure.CALC_CHECKSUM);
        const checksum = await this.expect(Procedure.CALC_CHECKSUM);
        corrupt = checksum.crc !== crc32(image.subarray(0, checksum.offset ?? 0));
      }

      if (!corrupt) {
        report('validating', end);
        await this.command(Procedure.EXECUTE);
        await this.expect(Procedure.EXECUTE, 120000);
        return;
      }

      if (attempt === MAX_ATTEMPTS) {
        throw new DfuError('The image kept arriving corrupted. Move the watch closer and retry.');
      }
    }
  }

  private async expect(procedure: number, timeoutMs?: number): Promise<SecureResponse> {
    const response = parseResponse(await this.notifications.next(timeoutMs));
    if (response.result !== SUCCESS) {
      const name = PROCEDURE_NAMES[response.procedure] ?? `procedure ${response.procedure}`;
      const result = RESULT_NAMES[response.result] ?? `error ${response.result}`;
      throw new DfuError(`The bootloader rejected ${name}: ${result}.`);
    }
    if (response.procedure !== procedure) {
      const expected = PROCEDURE_NAMES[procedure] ?? `procedure ${procedure}`;
      throw new DfuError(`The bootloader answered something other than ${expected}.`);
    }
    return response;
  }

  private command(procedure: number, params: Uint8Array | number[] = []): Promise<void> {
    return this.link.write(
      SECURE_DFU_SERVICE,
      SECURE_CONTROL_POINT,
      concat([procedure], params),
      'request',
    );
  }

  // Send an object's bytes, taking the receipt the bootloader sends on every
  // interval boundary so the next response is not read out of turn.
  private async stream(data: Uint8Array): Promise<void> {
    let segments = 0;
    for (let offset = 0; offset < data.length; offset += DFU_PAYLOAD_SIZE) {
      await this.packet(data.subarray(offset, offset + DFU_PAYLOAD_SIZE));
      segments += 1;
      if (segments % RECEIPT_INTERVAL === 0) {
        await this.expect(Procedure.CALC_CHECKSUM);
      }
    }
  }

  private packet(data: Uint8Array): Promise<void> {
    return this.link.write(SECURE_DFU_SERVICE, SECURE_PACKET, data, 'command');
  }
}
