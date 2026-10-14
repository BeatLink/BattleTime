// Reads a Nordic DFU zip: the manifest names an init packet (.dat) and an
// image (.bin) for each part of the firmware.

import { unzipSync } from 'fflate';

export type FirmwareImage = {
  // Which part of the firmware this is, as the manifest names it.
  part: string;
  init: Uint8Array;
  image: Uint8Array;
};

export type FirmwarePackage = {
  name: string;
  images: FirmwareImage[];
};

export class FirmwarePackageError extends Error {}

type ManifestEntry = { bin_file?: string; dat_file?: string };

// The order the bootloader expects when a zip carries more than one image.
const PART_ORDER = [
  'softdevice',
  'bootloader',
  'softdevice_bootloader',
  'application',
];

function partRank(part: string): number {
  const index = PART_ORDER.indexOf(part);
  return index === -1 ? PART_ORDER.length : index;
}

function take(files: Record<string, Uint8Array>, path: string, part: string): Uint8Array {
  const data = files[path];
  if (!data) {
    throw new FirmwarePackageError(`The package names ${path} for ${part} but does not contain it.`);
  }
  return data;
}

export function readFirmwarePackage(zip: Uint8Array, name = 'firmware.zip'): FirmwarePackage {
  let files: Record<string, Uint8Array>;
  try {
    files = unzipSync(zip);
  } catch {
    throw new FirmwarePackageError('That file is not a zip archive.');
  }

  const manifestFile = Object.keys(files).find((path) => path.endsWith('manifest.json'));
  const images: FirmwareImage[] = [];

  if (manifestFile) {
    let manifest: Record<string, ManifestEntry>;
    try {
      const text = new TextDecoder().decode(files[manifestFile]);
      manifest = (JSON.parse(text) as { manifest?: Record<string, ManifestEntry> }).manifest ?? {};
    } catch {
      throw new FirmwarePackageError('The package manifest is not valid JSON.');
    }
    for (const [part, entry] of Object.entries(manifest)) {
      if (!entry?.bin_file || !entry?.dat_file) {
        continue;
      }
      images.push({
        part,
        init: take(files, entry.dat_file, part),
        image: take(files, entry.bin_file, part),
      });
    }
    images.sort((a, b) => partRank(a.part) - partRank(b.part));
  } else {
    // Some hand-built zips carry only the pair of files.
    const dat = Object.keys(files).find((path) => path.endsWith('.dat'));
    const bin = Object.keys(files).find((path) => path.endsWith('.bin'));
    if (dat && bin) {
      images.push({ part: 'application', init: files[dat], image: files[bin] });
    }
  }

  if (images.length === 0) {
    throw new FirmwarePackageError('The package contains no firmware image.');
  }

  return { name, images };
}
