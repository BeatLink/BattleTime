// Nordic legacy DFU, which BattleTime's firmware speaks, as do the old systems' bootloaders a reloader goes through.

import { concat, u16le, u32le, readU32le } from './bytes';
import { DfuError, DfuRefusal, NotifyQueue, sleep, type DfuLink } from './link';
import type { FirmwareImage, FirmwarePart } from './package';
import {
  DFU_PAYLOAD_SIZE,
  LEGACY_CONTROL_POINT,
  LEGACY_DFU_SERVICE,
  LEGACY_PACKET,
} from './uuids';

export const Procedure = {
  START_DFU: 1,
  INITIALIZE_DFU: 2,
  RECEIVE_FIRMWARE_IMAGE: 3,
  VALIDATE_FIRMWARE: 4,
  ACTIVATE_IMAGE_AND_RESET: 5,
  RESET_SYSTEM: 6,
  REPORT_RECEIVED_IMAGE_SIZE: 7,
  PRN_REQUEST: 8,
  RESPONSE: 16,
  PACKET_RECEIPT_NOTIFICATION: 17,
} as const;

const PROCEDURE_NAMES: Record<number, string> = {
  1: 'start',
  2: 'initialise',
  3: 'receive image',
  4: 'validate',
  5: 'activate and reset',
  6: 'reset',
  7: 'report size',
  8: 'set notification interval',
};

const RESPONSE_NAMES: Record<number, string> = {
  1: 'success',
  2: 'invalid state',
  3: 'not supported',
  4: 'data size exceeds limits',
  5: 'CRC error',
  6: 'operation failed',
};

const SUCCESS = 1;

// Which image type the START_DFU command asks the watch to expect.
const IMAGE_TYPE: Record<FirmwarePart, number> = {
  application: 0x04,
  // BattleTime's own type: the running firmware stores the image in its recovery slot and keeps running.
  recovery: 0x08,
};

// The bootloader answers once every this many packets, which keeps the radio
// busy without letting the watch fall behind.
const RECEIPT_INTERVAL = 5;

export type ParsedNotification =
  | { kind: 'response'; procedure: number; result: number; value: Uint8Array }
  | { kind: 'receipt'; received: number };

export function parseNotification(value: Uint8Array): ParsedNotification {
  if (value.length < 3) {
    throw new DfuError('The bootloader sent a truncated notification.');
  }
  if (value[0] === Procedure.PACKET_RECEIPT_NOTIFICATION) {
    return { kind: 'receipt', received: readU32le(value, 1) };
  }
  if (value[0] !== Procedure.RESPONSE) {
    throw new DfuError(`Unexpected notification opcode 0x${value[0].toString(16)}.`);
  }
  return { kind: 'response', procedure: value[1], result: value[2], value };
}

export type DfuProgress = {
  part: FirmwarePart;
  phase: 'preparing' | 'sending' | 'validating' | 'done';
  sent: number;
  total: number;
};

export type DfuOptions = {
  // Ask a BattleTime watch how much of this image it kept from an upload a lost link cut off, and send only the rest.
  resume?: boolean;
};

export class LegacyDfuController {
  private readonly notifications = new NotifyQueue();
  private unsubscribe: (() => void) | null = null;

  constructor(private readonly link: DfuLink) {}

  async run(
    image: FirmwareImage,
    onProgress?: (progress: DfuProgress) => void,
    options: DfuOptions = {},
  ): Promise<void> {
    const total = image.image.length;
    const report = (phase: DfuProgress['phase'], sent: number) =>
      onProgress?.({ part: image.part, phase, sent, total });

    report('preparing', 0);
    this.unsubscribe = await this.link.subscribe(
      LEGACY_DFU_SERVICE,
      LEGACY_CONTROL_POINT,
      (value) => this.notifications.push(value),
    );

    try {
      const type = IMAGE_TYPE[image.part];
      await this.command(Procedure.START_DFU, [type]);

      // The watch expects softdevice, bootloader and application sizes, in that order; only the last is sent.
      const sizes = new Uint8Array(12);
      sizes.set(u32le(total), 8);
      await this.packet(sizes);
      await this.expect(Procedure.START_DFU);

      await this.command(Procedure.INITIALIZE_DFU, [0x00]);
      await this.packet(image.init);
      await this.command(Procedure.INITIALIZE_DFU, [0x01]);
      // Answered once the bootloader has finished erasing flash.
      await this.expect(Procedure.INITIALIZE_DFU, 120000);

      const start = options.resume ? await this.keptBytes(total) : 0;

      await this.command(Procedure.PRN_REQUEST, u16le(RECEIPT_INTERVAL));
      await this.command(Procedure.RECEIVE_FIRMWARE_IMAGE);
      report('sending', start);

      let segments = 0;
      for (let offset = start; offset < total; offset += DFU_PAYLOAD_SIZE) {
        await this.packet(image.image.subarray(offset, offset + DFU_PAYLOAD_SIZE));
        segments += 1;
        // The bootloader answers on every interval boundary, including the
        // last one, so always take the receipt it sent.
        if (segments % RECEIPT_INTERVAL === 0) {
          const notification = parseNotification(await this.notifications.next());
          if (notification.kind !== 'receipt') {
            throw this.failure(notification);
          }
          report('sending', notification.received);
        }
      }

      report('sending', total);
      // Answered once the whole image has landed.
      await this.expect(Procedure.RECEIVE_FIRMWARE_IMAGE, 120000);

      report('validating', total);
      await this.command(Procedure.VALIDATE_FIRMWARE);
      await this.expect(Procedure.VALIDATE_FIRMWARE, 120000);

      // The bootloader is still copying the image when it answers.
      await sleep(1000);
      await this.command(Procedure.ACTIVATE_IMAGE_AND_RESET);
      report('done', total);
    } finally {
      this.unsubscribe?.();
      this.unsubscribe = null;
    }
  }

  // How much of this image the watch kept from a cut-off upload; it answers 0 for a different image.
  private async keptBytes(total: number): Promise<number> {
    await this.command(Procedure.REPORT_RECEIVED_IMAGE_SIZE);
    const notification = parseNotification(await this.notifications.next());
    if (
      notification.kind !== 'response' ||
      notification.procedure !== Procedure.REPORT_RECEIVED_IMAGE_SIZE ||
      notification.result !== SUCCESS ||
      notification.value.length < 7
    ) {
      throw this.failure(notification);
    }
    const kept = readU32le(notification.value, 3);
    // The watch now waits for the bytes after what it kept, so a place no packet starts at cannot be sent.
    if (kept % DFU_PAYLOAD_SIZE !== 0 || kept > total) {
      throw new DfuError(`The watch kept ${kept} bytes, which is not a place this image can carry on from.`);
    }
    return kept;
  }

  private failure(notification: ParsedNotification): DfuError {
    if (notification.kind === 'receipt') {
      return new DfuError('The bootloader sent a packet receipt out of turn.');
    }
    const procedure = PROCEDURE_NAMES[notification.procedure] ?? `procedure ${notification.procedure}`;
    const result = RESPONSE_NAMES[notification.result] ?? `error ${notification.result}`;
    return new DfuRefusal(`The bootloader rejected ${procedure}: ${result}.`);
  }

  private async expect(procedure: number, timeoutMs?: number): Promise<void> {
    const notification = parseNotification(await this.notifications.next(timeoutMs));
    if (notification.kind !== 'response' || notification.result !== SUCCESS) {
      throw this.failure(notification);
    }
    if (notification.procedure !== procedure) {
      const expected = PROCEDURE_NAMES[procedure] ?? `procedure ${procedure}`;
      throw new DfuError(`The bootloader answered something other than ${expected}.`);
    }
  }

  private command(procedure: number, params: Uint8Array | number[] = []): Promise<void> {
    return this.link.write(
      LEGACY_DFU_SERVICE,
      LEGACY_CONTROL_POINT,
      concat([procedure], params),
      'request',
    );
  }

  private async packet(data: Uint8Array): Promise<void> {
    for (let offset = 0; offset < data.length; offset += DFU_PAYLOAD_SIZE) {
      await this.link.write(
        LEGACY_DFU_SERVICE,
        LEGACY_PACKET,
        data.subarray(offset, offset + DFU_PAYLOAD_SIZE),
        'command',
      );
    }
  }
}
