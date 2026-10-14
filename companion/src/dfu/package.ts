// Reads a Nordic DFU zip: the manifest names the init packet (.dat) and the
// image (.bin) of the application, the only part a NeoTime update carries.

import { unzipSync } from 'fflate';

// The image type the controller asks the watch for: an ordinary update, or NeoTime's recovery slot.
export type FirmwarePart = 'application' | 'recovery';

export type FirmwareImage = {
  part: FirmwarePart;
  init: Uint8Array;
  image: Uint8Array;
};

export type FirmwarePackage = {
  name: string;
  image: FirmwareImage;
};

export class FirmwarePackageError extends Error {}

type ManifestEntry = { bin_file?: string; dat_file?: string };

function take(files: Record<string, Uint8Array>, path: string): Uint8Array {
  const data = files[path];
  if (!data) {
    throw new FirmwarePackageError(`The package names ${path} but does not contain it.`);
  }
  return data;
}

function readManifest(files: Record<string, Uint8Array>, manifestFile: string): Record<string, ManifestEntry> {
  try {
    const text = new TextDecoder().decode(files[manifestFile]);
    return (JSON.parse(text) as { manifest?: Record<string, ManifestEntry> }).manifest ?? {};
  } catch {
    throw new FirmwarePackageError('The package manifest is not valid JSON.');
  }
}

export function readFirmwarePackage(zip: Uint8Array, name = 'firmware.zip'): FirmwarePackage {
  let files: Record<string, Uint8Array>;
  try {
    files = unzipSync(zip);
  } catch {
    throw new FirmwarePackageError('That file is not a zip archive.');
  }

  const manifestFile = Object.keys(files).find((path) => path.endsWith('manifest.json'));
  if (manifestFile) {
    const manifest = readManifest(files, manifestFile);
    const others = Object.keys(manifest).filter((part) => part !== 'application');
    if (others.length > 0) {
      throw new FirmwarePackageError(`NeoTime takes only an application image, and this package holds ${others.join(', ')}.`);
    }
    const entry = manifest.application;
    if (entry?.bin_file && entry?.dat_file) {
      return {
        name,
        image: { part: 'application', init: take(files, entry.dat_file), image: take(files, entry.bin_file) },
      };
    }
  } else {
    // Some hand-built zips carry only the pair of files.
    const dat = Object.keys(files).find((path) => path.endsWith('.dat'));
    const bin = Object.keys(files).find((path) => path.endsWith('.bin'));
    if (dat && bin) {
      return { name, image: { part: 'application', init: files[dat], image: files[bin] } };
    }
  }

  throw new FirmwarePackageError('The package contains no firmware image.');
}
