// Setting up a new watch: NeoTime ships with no apps, so the companion installs a watch face, the launcher
// and the apps it bundles. The shared library comes inside the firmware, so it is never sent.

import { base64ToBytes } from '@/ble/encoding';
import { BUNDLED_IMAGES, type BundledImage } from '@/bundles/watch-apps';

import { readImageHeader, type ImageHeader } from './app-image';
import { DEFAULT_LAUNCHER, writeLauncher } from './launcher';
import { installImage, type FileWriter, type PackagesClient, type WatchApp } from './packages';

export type SetupItem = { file: string; header: ImageHeader; image: Uint8Array; required: boolean };

export type SetupStep = { item: SetupItem; index: number; count: number; sent: number; total: number };

// The launcher is an ordinary app with this id, which the watch opens with a swipe up the face.
export const LAUNCHER_ID = 'launcher';

// The face and the launcher go first, then the rest.
function rank(item: SetupItem): number {
  if (item.header.kind === 'face') return 0;
  if (item.header.id === LAUNCHER_ID) return 1;
  return 2;
}

export function setupItems(bundle: BundledImage[] = BUNDLED_IMAGES): SetupItem[] {
  return bundle
    .map(({ file, base64 }) => {
      const image = base64ToBytes(base64);
      const header = readImageHeader(image);
      const required = header.kind !== 'app' || header.id === LAUNCHER_ID;
      return { file, header, image, required };
    })
    .filter((item) => item.header.kind !== 'library')
    .sort((a, b) => rank(a) - rank(b) || a.header.name.localeCompare(b.header.name));
}

// A watch with no face shows only its help screen, which is when to offer the setup.
export function needsSetup(apps: WatchApp[]): boolean {
  return !apps.some((app) => app.kind === 'face');
}

// Installs each chosen item in order, reporting progress, and stops at the first failure. When the launcher was
// among them it starts with the default groups, as folders; firmware without groups ignores the file.
export async function runSetup(
  files: FileWriter,
  packages: PackagesClient,
  items: SetupItem[],
  onStep?: (step: SetupStep) => void,
): Promise<void> {
  for (const [index, item] of items.entries()) {
    onStep?.({ item, index, count: items.length, sent: 0, total: item.image.length });
    await installImage(files, packages, item.image, (progress) =>
      onStep?.({ item, index, count: items.length, sent: progress.sent, total: progress.total }),
    );
  }
  if (items.some((item) => item.header.id === LAUNCHER_ID)) {
    await writeLauncher(files, packages, DEFAULT_LAUNCHER).catch(() => {});
  }
}
