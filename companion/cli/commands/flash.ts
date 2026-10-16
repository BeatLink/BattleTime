// Send a firmware package to the watch, the same way the Firmware tab does.

import { readFileSync } from 'node:fs';
import { basename } from 'node:path';

import { ENTER_DFU_COMMAND, flashFirmware } from '@/dfu/flash';
import { readFirmwarePackage } from '@/dfu/package';
import { setWatchTime } from '@/protocol/battletime';
import { putMissingSystemFiles } from '@/protocol/system-files';

import type { WatchSession } from '../ble';
import { kilobytes, ProgressBar, say } from '../output';

export const flashUsage = `battletime flash <package.zip> [--address <mac>]

  Sends a Nordic DFU package, such as the zip tools/package_dfu.py makes in
  the BattleTime tree. BattleTime takes it in its running firmware, restarts, and
  keeps it once this tool reconnects and sets the time. The tool then puts the
  files the firmware reads that are not apps, such as the accelerometer's
  configuration and the icons, if the watch lacks them. A watch still on wasp-os, taking
  BattleTime's reloader, is first restarted into its bootloader.`;

export async function flash(session: WatchSession, path: string): Promise<void> {
  const firmware = readFirmwarePackage(new Uint8Array(readFileSync(path)), basename(path));
  const total = firmware.image.image.length;

  say(`${firmware.name}: ${kilobytes(total)}`);

  const bar = new ProgressBar();
  let confirmed = false;
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
          confirmed = step.confirmed;
          say(
            step.confirmed
              ? 'The new firmware is running, and the watch keeps it.'
              : 'The watch did not come back. Connect to it once it restarts, or it goes back to the old firmware.',
          );
          break;
      }
    },
  });
  if (confirmed) {
    await systemFiles(session).catch((error: unknown) =>
      say(
        `Could not put the system files: ${error instanceof Error ? error.message : String(error)}. ` +
          'Run `battletime system-files`, or connect the companion app, which puts them.',
      ),
    );
  }
}

export const systemFilesUsage = `battletime system-files

  Puts the files the firmware reads that are not apps, such as the
  accelerometer's configuration and the icons, on the watch if it lacks them.
  Without its configuration the watch counts no steps, and without the icons
  it draws empty boxes in their place.`;

export async function systemFiles(session: WatchSession): Promise<void> {
  const written = await putMissingSystemFiles(await session.files());
  say(written.length > 0 ? `Put ${written.join(', ')} on the watch.` : 'The watch has every system file.');
}
