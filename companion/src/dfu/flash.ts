// Runs a whole firmware update over Nordic legacy DFU, then reconnects so the watch keeps the new firmware.
//
// BattleTime takes the update in the running firmware, at the watch's own address. A watch still on wasp-os
// takes BattleTime's reloader in its bootloader instead, which it reaches over the UART line and which
// answers at the address one above the watch's.

import { nextAddress } from './bytes';
import { DfuError, DfuRefusal, sleep, type DfuLink } from './link';
import { LegacyDfuController, type DfuProgress } from './legacy';
import type { FirmwareImage, FirmwarePackage } from './package';
import { LEGACY_CONTROL_POINT, LEGACY_DFU_SERVICE } from './uuids';

export type FlashStep =
  | { step: 'resetting' }
  | { step: 'connecting' }
  | { step: 'detected' }
  | { step: 'progress'; progress: DfuProgress }
  | { step: 'reconnecting'; attempt: number; attempts: number }
  | { step: 'confirming' }
  | { step: 'finished'; confirmed: boolean };

export type FlashOptions = {
  // Reset a watch still on wasp-os, which serves no DFU itself, into its bootloader to take the reloader.
  enterDfu?: () => Promise<void>;
  // Exchange data with the watch once it is back on the new firmware; BattleTime rolls an update back
  // at its next reset unless a phone does this.
  confirm?: (link: DfuLink) => Promise<void>;
  onStep?: (step: FlashStep) => void;
  // How long to wait after a lost link before reconnecting to carry on.
  resumePauseMs?: number;
};

// The line wasptool sends to make wasp-os restart into its bootloader.
export const ENTER_DFU_COMMAND = 'import machine\r\nmachine.enter_ota_dfu()\r\n';

// How long a watch takes to restart and start advertising again.
const REBOOT_MS = 3000;

// How long the bootloader may take to swap the new image in before the firmware advertises.
const SWAP_MS = 20000;

// How many times a lost link is reconnected to carry on with an upload, and how long to wait first.
const RESUME_ATTEMPTS = 5;
const RESUME_PAUSE_MS = 2000;

type CarryOnOptions = Pick<FlashOptions, 'onStep' | 'resumePauseMs'>;

function hasDfu(link: DfuLink): Promise<boolean> {
  return link.hasCharacteristic(LEGACY_DFU_SERVICE, LEGACY_CONTROL_POINT);
}

// The watch's own address first, then the one above it, where wasp-os's bootloader answers.
function candidateAddresses(address: string): string[] {
  try {
    return [address, nextAddress(address)];
  } catch {
    // Not an address one can be derived from, such as the handle iOS gives instead.
    return [address];
  }
}

async function reconnectToDfu(link: DfuLink, address: string): Promise<boolean> {
  let lastError: unknown = null;
  for (const candidate of candidateAddresses(address)) {
    try {
      await link.reconnect(candidate);
    } catch (error) {
      lastError = error;
      continue;
    }
    if (await hasDfu(link)) {
      return true;
    }
  }
  if (lastError) {
    throw lastError;
  }
  return false;
}

// Keep trying the watch's own address until the swapped-in firmware answers.
async function reconnectAfterSwap(link: DfuLink, address: string): Promise<boolean> {
  const deadline = Date.now() + SWAP_MS;
  while (Date.now() < deadline) {
    try {
      await link.reconnect(address);
      return true;
    } catch {
      await sleep(1000);
    }
  }
  return false;
}

// Sends an image to a watch running BattleTime, reconnecting after a lost link to carry on where the watch stopped.
//
// A refusal from the watch ends it at once, since sending the same image again would be refused again.
async function sendCarryingOn(
  link: DfuLink,
  address: string,
  image: FirmwareImage,
  { onStep, resumePauseMs = RESUME_PAUSE_MS }: CarryOnOptions,
): Promise<void> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      await new LegacyDfuController(link).run(image, (progress) => onStep?.({ step: 'progress', progress }), {
        resume: true,
      });
      return;
    } catch (error) {
      if (error instanceof DfuRefusal || attempt >= RESUME_ATTEMPTS) {
        throw error;
      }
      onStep?.({ step: 'reconnecting', attempt, attempts: RESUME_ATTEMPTS - 1 });
      await sleep(resumePauseMs);
      // A reconnect that fails leaves the next attempt to fail fast and try again.
      await link.reconnect(address).catch(() => undefined);
    }
  }
}

// Sends a firmware package to BattleTime's recovery slot, which the bootloader menu installs from.
//
// The running firmware takes it with DFU image type 0x08 and stays where it is, so nothing is swapped
// and nothing needs keeping; it restarts, so the time is written again once it is back.
export async function flashRecovery(
  link: DfuLink,
  firmware: FirmwarePackage,
  options: Pick<FlashOptions, 'confirm' | 'onStep' | 'resumePauseMs'> = {},
): Promise<void> {
  const { onStep } = options;
  if (!(await hasDfu(link))) {
    throw new DfuError('Only a watch running BattleTime keeps a recovery firmware.');
  }
  onStep?.({ step: 'detected' });

  const image = { ...firmware.image, part: 'recovery' as const };
  await sendCarryingOn(link, link.deviceId, image, options);

  let confirmed = false;
  if (options.confirm) {
    onStep?.({ step: 'confirming' });
    await sleep(REBOOT_MS);
    if (await reconnectAfterSwap(link, link.deviceId)) {
      await options.confirm(link);
      confirmed = true;
    }
  }
  onStep?.({ step: 'finished', confirmed });
}

export async function flashFirmware(
  link: DfuLink,
  firmware: FirmwarePackage,
  options: FlashOptions = {},
): Promise<void> {
  const { onStep } = options;
  const address = link.deviceId;
  const runsBattleTime = await hasDfu(link);

  if (!runsBattleTime) {
    if (!options.enterDfu) {
      throw new DfuError('This watch serves no firmware update and has no way into a bootloader.');
    }
    onStep?.({ step: 'resetting' });
    await options.enterDfu();
    await sleep(REBOOT_MS);
    onStep?.({ step: 'connecting' });
    if (!(await reconnectToDfu(link, address))) {
      throw new DfuError('The watch did not come back speaking Nordic DFU.');
    }
  }
  onStep?.({ step: 'detected' });

  if (runsBattleTime) {
    await sendCarryingOn(link, address, firmware.image, options);
  } else {
    // An old system's bootloader keeps nothing of a cut-off upload, so it gets one try.
    await new LegacyDfuController(link).run(firmware.image, (progress) => onStep?.({ step: 'progress', progress }));
  }

  let confirmed = false;
  if (options.confirm) {
    onStep?.({ step: 'confirming' });
    await sleep(REBOOT_MS);
    if (await reconnectAfterSwap(link, address)) {
      await options.confirm(link);
      confirmed = true;
    }
  }
  onStep?.({ step: 'finished', confirmed });
}
