// Drives a firmware update from the UI: pick a package, send it, and report
// where the update has got to.

import * as DocumentPicker from 'expo-document-picker';
import { useCallback, useState } from 'react';

import { ENTER_DFU_COMMAND, flashFirmware, type FlashStep } from '@/dfu/flash';
import type { DfuLink } from '@/dfu/link';
import { readFirmwarePackage, type FirmwarePackage } from '@/dfu/package';

export type FirmwareState = {
  firmware: FirmwarePackage | null;
  // What the update is doing, ready to show as it is.
  status: string | null;
  sent: number;
  total: number;
  flashing: boolean;
  finished: boolean;
  error: string | null;
  choose: () => Promise<void>;
  flash: () => Promise<void>;
  clearError: () => void;
};

function describe(step: FlashStep): string {
  switch (step.step) {
    case 'resetting':
      return 'Restarting the watch in its bootloader';
    case 'connecting':
      return 'Waiting for the bootloader';
    case 'detected':
      return step.flavour === 'secure'
        ? 'Talking to a secure bootloader'
        : 'Talking to a legacy bootloader';
    case 'progress': {
      const part = step.images > 1 ? `${step.progress.part} (${step.image} of ${step.images})` : step.progress.part;
      switch (step.progress.phase) {
        case 'preparing':
          return `Preparing ${part}`;
        case 'sending':
          return `Sending ${part}`;
        case 'validating':
          return `Checking ${part}`;
        case 'done':
          return `Sent ${part}`;
      }
      return `Sending ${part}`;
    }
    case 'finished':
      return 'The watch is restarting on the new firmware';
  }
}

async function readPicked(uri: string): Promise<Uint8Array> {
  const response = await fetch(uri);
  return new Uint8Array(await response.arrayBuffer());
}

export function useFirmware(link: () => DfuLink | null, sendRaw: (text: string) => Promise<void>) {
  const [firmware, setFirmware] = useState<FirmwarePackage | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [sent, setSent] = useState(0);
  const [total, setTotal] = useState(0);
  const [flashing, setFlashing] = useState(false);
  const [finished, setFinished] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const choose = useCallback(async () => {
    setError(null);
    const picked = await DocumentPicker.getDocumentAsync({
      type: ['application/zip', 'application/octet-stream', '*/*'],
      copyToCacheDirectory: true,
    });
    if (picked.canceled || !picked.assets?.[0]) {
      return;
    }
    const asset = picked.assets[0];
    try {
      const parsed = readFirmwarePackage(await readPicked(asset.uri), asset.name);
      setFirmware(parsed);
      setFinished(false);
      setStatus(null);
      setSent(0);
      setTotal(parsed.images.reduce((sum, image) => sum + image.image.length, 0));
    } catch (caught) {
      setFirmware(null);
      setError((caught as Error).message);
    }
  }, []);

  const flash = useCallback(async () => {
    const dfu = link();
    if (!dfu || !firmware) {
      setError('Connect to a watch and pick a firmware package first.');
      return;
    }

    setFlashing(true);
    setFinished(false);
    setError(null);
    setSent(0);

    // Every image in the package counts towards the one bar.
    const sizes = firmware.images.map((image) => image.image.length);
    try {
      await flashFirmware(dfu, firmware, {
        enterDfu: () => sendRaw(ENTER_DFU_COMMAND),
        onStep: (step) => {
          setStatus(describe(step));
          if (step.step === 'progress') {
            const before = sizes.slice(0, step.image - 1).reduce((sum, size) => sum + size, 0);
            setSent(before + step.progress.sent);
          }
          if (step.step === 'finished') {
            setFinished(true);
          }
        },
      });
    } catch (caught) {
      setError((caught as Error).message);
      setStatus(null);
    } finally {
      setFlashing(false);
    }
  }, [firmware, link, sendRaw]);

  const clearError = useCallback(() => setError(null), []);

  return { firmware, status, sent, total, flashing, finished, error, choose, flash, clearError };
}
