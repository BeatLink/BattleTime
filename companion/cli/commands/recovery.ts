// Send a recovery firmware to the watch's recovery slot, for the bootloader menu to install.

import { readFileSync } from 'node:fs';
import { basename } from 'node:path';

import { flashRecovery } from '@/dfu/flash';
import { readFirmwarePackage } from '@/dfu/package';
import { setWatchTime } from '@/protocol/neotime';

import type { WatchSession } from '../ble';
import { kilobytes, ProgressBar, say } from '../output';

export const recoveryUsage = `neotime recovery <package.zip> [--address <mac>]

  Sends a recovery firmware, such as dist/recovery.zip from the NeoTime
  tree, to the watch's recovery slot. The running firmware stays; the
  bootloader menu's Install recovery puts the new one in when it is needed.`;

export async function recovery(session: WatchSession, path: string): Promise<void> {
  const firmware = readFirmwarePackage(new Uint8Array(readFileSync(path)), basename(path));
  const total = firmware.images.reduce((sum, image) => sum + image.image.length, 0);
  say(`${firmware.name}: recovery firmware, ${kilobytes(total)}`);

  const bar = new ProgressBar();
  await flashRecovery(session, firmware, {
    confirm: (link) => setWatchTime(link),
    onStep: (step) => {
      switch (step.step) {
        case 'progress':
          bar.update(step.progress.sent, total, 'recovery');
          break;
        case 'confirming':
          bar.done();
          say('Stored. Waiting for the watch to restart...');
          break;
        case 'finished':
          say(
            step.confirmed
              ? 'The recovery slot holds the new recovery firmware, and the watch is back.'
              : 'The recovery firmware was sent, but the watch did not come back to have its time set.',
          );
          break;
        default:
          break;
      }
    },
  });
}
