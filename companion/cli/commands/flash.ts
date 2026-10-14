// Send a firmware package to the watch, the same way the Firmware tab does.

import { readFileSync } from 'node:fs';
import { basename } from 'node:path';

import { ENTER_DFU_COMMAND, flashFirmware } from '@/dfu/flash';
import { readFirmwarePackage } from '@/dfu/package';
import { setWatchTime } from '@/protocol/neotime';

import type { WatchSession } from '../ble';
import { kilobytes, ProgressBar, say } from '../output';

export const flashUsage = `neotime flash <package.zip> [--address <mac>]

  Sends a Nordic DFU package, such as the zip tools/package_dfu.py makes in
  the NeoTime tree. NeoTime takes it in its running firmware, restarts, and
  keeps it once this tool reconnects and sets the time. A watch still on
  wasp-os, taking NeoTime's reloader, is first restarted into its bootloader.`;

export async function flash(session: WatchSession, path: string): Promise<void> {
  const firmware = readFirmwarePackage(new Uint8Array(readFileSync(path)), basename(path));
  const total = firmware.image.image.length;

  say(`${firmware.name}: ${kilobytes(total)}`);

  const bar = new ProgressBar();
  await flashFirmware(session, firmware, {
    enterDfu: () => session.send(ENTER_DFU_COMMAND),
    confirm: (link) => setWatchTime(link),
    onStep: (step) => {
      switch (step.step) {
        case 'resetting':
          say('Restarting wasp-os in its bootloader...');
          break;
        case 'connecting':
          say('Waiting for the bootloader...');
          break;
        case 'detected':
          break;
        case 'progress':
          bar.update(step.progress.sent, total, step.progress.part);
          break;
        case 'reconnecting':
          bar.done();
          say(`Lost the watch; reconnecting to carry on (try ${step.attempt} of ${step.attempts})...`);
          break;
        case 'confirming':
          bar.done();
          say('Sent. Waiting for the new firmware to start...');
          break;
        case 'finished':
          say(
            step.confirmed
              ? 'The new firmware is running, and the watch keeps it.'
              : 'The watch did not come back. Connect to it once it restarts, or it goes back to the old firmware.',
          );
          break;
      }
    },
  });
}
