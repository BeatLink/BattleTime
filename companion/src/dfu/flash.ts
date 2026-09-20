// Runs a whole firmware update: get the watch into its bootloader, work out
// which DFU protocol that bootloader speaks, then send every image in the
// package.

import { nextAddress } from './bytes';
import { DfuError, sleep, type DfuLink } from './link';
import { LegacyDfuController, type DfuProgress } from './legacy';
import type { FirmwarePackage } from './package';
import { SecureDfuController } from './secure';
import {
  LEGACY_CONTROL_POINT,
  LEGACY_DFU_SERVICE,
  SECURE_BUTTONLESS,
  SECURE_CONTROL_POINT,
  SECURE_DFU_SERVICE,
} from './uuids';

export type DfuFlavour = 'legacy' | 'secure';

export type FlashStep =
  | { step: 'resetting' }
  | { step: 'connecting' }
  | { step: 'detected'; flavour: DfuFlavour }
  | { step: 'progress'; progress: DfuProgress; image: number; images: number }
  | { step: 'finished' };

export type FlashOptions = {
  // Reset the watch out of its application and into the bootloader over the
  // UART line, the way wasptool does. Skipped when the bootloader is already
  // running.
  enterDfu?: () => Promise<void>;
  onStep?: (step: FlashStep) => void;
};

// The line wasptool sends to leave the application and come back in the
// bootloader.
export const ENTER_DFU_COMMAND = 'import machine\r\nmachine.enter_ota_dfu()\r\n';

// How long the watch takes to reboot into its bootloader and start
// advertising again.
const REBOOT_MS = 3000;

// A reset brings a legacy bootloader back at the watch's own address and a
// secure one at the next address up, so try both before giving up.
async function reconnectToBootloader(link: DfuLink): Promise<DfuFlavour | null> {
  const addresses = [link.deviceId];
  try {
    addresses.push(nextAddress(link.deviceId));
  } catch {
    // Not an address the bootloader's can be derived from, such as the iOS
    // handle react-native-ble-plx gives instead.
  }

  let lastError: unknown = null;
  for (const address of addresses) {
    try {
      await link.reconnect(address);
    } catch (error) {
      lastError = error;
      continue;
    }
    const flavour = await detect(link);
    if (flavour) {
      return flavour;
    }
  }
  if (lastError) {
    throw lastError;
  }
  return null;
}

async function detect(link: DfuLink): Promise<DfuFlavour | null> {
  if (await link.hasCharacteristic(SECURE_DFU_SERVICE, SECURE_CONTROL_POINT)) {
    return 'secure';
  }
  if (await link.hasCharacteristic(LEGACY_DFU_SERVICE, LEGACY_CONTROL_POINT)) {
    return 'legacy';
  }
  return null;
}

export async function flashFirmware(
  link: DfuLink,
  firmware: FirmwarePackage,
  options: FlashOptions = {},
): Promise<void> {
  const { onStep } = options;

  let flavour = await detect(link);
  const inBootloader =
    (flavour === 'secure' && (await SecureDfuController.inDfuMode(link))) ||
    (flavour === 'legacy' && (await LegacyDfuController.inDfuMode(link)));

  if (!inBootloader) {
    onStep?.({ step: 'resetting' });
    if (options.enterDfu) {
      await options.enterDfu();
      await sleep(REBOOT_MS);
      onStep?.({ step: 'connecting' });
      flavour = await reconnectToBootloader(link);
    } else if (await link.hasCharacteristic(SECURE_DFU_SERVICE, SECURE_BUTTONLESS)) {
      await SecureDfuController.switchToDfuMode(link);
      flavour = await detect(link);
    } else if (flavour === 'legacy') {
      await LegacyDfuController.switchToDfuMode(link);
      flavour = await detect(link);
    } else {
      throw new DfuError('This watch exposes no way into its bootloader.');
    }
  }

  if (!flavour) {
    throw new DfuError('The watch is not running a bootloader that speaks Nordic DFU.');
  }
  onStep?.({ step: 'detected', flavour });

  const controller =
    flavour === 'secure' ? new SecureDfuController(link) : new LegacyDfuController(link);

  for (const [index, image] of firmware.images.entries()) {
    await controller.run(image, (progress) =>
      onStep?.({ step: 'progress', progress, image: index + 1, images: firmware.images.length }),
    );
  }

  onStep?.({ step: 'finished' });
}
