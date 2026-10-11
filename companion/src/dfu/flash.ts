// Runs a whole firmware update over Nordic legacy DFU, then reconnects so the watch keeps the new firmware.
//
// NeoTime takes the update in the running firmware, at the watch's own address. A watch still on wasp-os
// takes it in its bootloader instead, which it reaches over the UART line and which answers at the
// address one above the watch's.

import { nextAddress } from './bytes';
import { DfuError, sleep, type DfuLink } from './link';
import { LegacyDfuController, type DfuProgress } from './legacy';
import type { FirmwarePackage } from './package';
import { LEGACY_CONTROL_POINT, LEGACY_DFU_SERVICE } from './uuids';

export type FlashStep =
  | { step: 'resetting' }
  | { step: 'connecting' }
  | { step: 'detected' }
  | { step: 'progress'; progress: DfuProgress; image: number; images: number }
  | { step: 'confirming' }
  | { step: 'finished'; confirmed: boolean };

export type FlashOptions = {
  // Reset a watch that serves no DFU itself into its bootloader, the way wasptool does on wasp-os.
  enterDfu?: () => Promise<void>;
  // Exchange data with the watch once it is back on the new firmware; NeoTime rolls an update back
  // at its next reset unless a phone does this.
  confirm?: (link: DfuLink) => Promise<void>;
  onStep?: (step: FlashStep) => void;
};

// The line wasptool sends to make wasp-os restart into its bootloader.
export const ENTER_DFU_COMMAND = 'import machine\r\nmachine.enter_ota_dfu()\r\n';

// How long a watch takes to restart and start advertising again.
const REBOOT_MS = 3000;

// How long the bootloader may take to swap the new image in before the firmware advertises.
const SWAP_MS = 20000;

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

export async function flashFirmware(
  link: DfuLink,
  firmware: FirmwarePackage,
  options: FlashOptions = {},
): Promise<void> {
  const { onStep } = options;
  const address = link.deviceId;

  if (!(await hasDfu(link))) {
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

  const controller = new LegacyDfuController(link);
  for (const [index, image] of firmware.images.entries()) {
    await controller.run(image, (progress) =>
      onStep?.({ step: 'progress', progress, image: index + 1, images: firmware.images.length }),
    );
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
