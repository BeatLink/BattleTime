// Send a firmware package to the watch, the same way the Firmware tab does.

import { readFileSync } from 'node:fs';
import { basename } from 'node:path';

import { ENTER_DFU_COMMAND, flashFirmware } from '@/dfu/flash';
import { readFirmwarePackage } from '@/dfu/package';

import type { WatchSession } from '../ble';
import { kilobytes, ProgressBar, say } from '../output';

export const flashUsage = `waspos flash <package.zip> [--address <mac>]

  Sends a Nordic DFU package to the watch. The watch restarts in its
  bootloader, takes the new firmware and restarts again.`;

export async function flash(session: WatchSession, path: string): Promise<void> {
  const firmware = readFirmwarePackage(new Uint8Array(readFileSync(path)), basename(path));
  const sizes = firmware.images.map((image) => image.image.length);
  const total = sizes.reduce((sum, size) => sum + size, 0);

  say(`${firmware.name}: ${firmware.images.map((image) => image.part).join(', ')}, ${kilobytes(total)}`);

  const bar = new ProgressBar();
  await flashFirmware(session, firmware, {
    enterDfu: () => session.send(ENTER_DFU_COMMAND),
    onStep: (step) => {
      switch (step.step) {
        case 'resetting':
          say('Restarting the watch in its bootloader...');
          break;
        case 'connecting':
          say('Waiting for the bootloader...');
          break;
        case 'detected':
          say(`Found a ${step.flavour} bootloader.`);
          break;
        case 'progress': {
          const before = sizes.slice(0, step.image - 1).reduce((sum, size) => sum + size, 0);
          bar.update(before + step.progress.sent, total, step.progress.part);
          break;
        }
        case 'finished':
          bar.done();
          say('Sent. The watch is restarting on the new firmware.');
          break;
      }
    },
  });
}
