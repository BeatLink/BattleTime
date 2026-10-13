import { describe, expect, it } from '@jest/globals';

import { bytesToBase64 } from '@/ble/encoding';
import { BUNDLED_IMAGES } from '@/bundles/watch-apps';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { testImage } from '@/test-support/app-images';

import { PackagesClient, type FileWriter } from './packages';
import { DEFAULT_LAUNCHER, LAUNCHER_PATH, readLauncherFile } from './launcher';
import { installBundledLibrary, needsLibrary, needsSetup, runSetup, setupItems, type SetupStep } from './setup';

const bundle = [
  { file: 'timer.ntapp', base64: bytesToBase64(testImage('timer', 'Timer')) },
  { file: 'launcher.ntapp', base64: bytesToBase64(testImage('launcher', 'Launcher')) },
  { file: 'face.ntapp', base64: bytesToBase64(testImage('digital', 'Digital', 'face')) },
  { file: 'shared.ntlib', base64: bytesToBase64(testImage('shared', 'Shared library', 'library')) },
  { file: 'dice.ntapp', base64: bytesToBase64(testImage('dice', 'Dice')) },
];

describe('setting up a new watch', () => {
  it('orders the library, the face and the launcher before the apps, and requires those three', () => {
    const items = setupItems(bundle);
    expect(items.map((item) => item.header.id)).toEqual(['shared', 'digital', 'launcher', 'dice', 'timer']);
    expect(items.map((item) => item.required)).toEqual([true, true, true, false, false]);
  });

  it('reads every image this app bundles', () => {
    const items = setupItems(BUNDLED_IMAGES);
    expect(items.length).toBe(BUNDLED_IMAGES.length);
    expect(items.some((item) => item.header.kind === 'face')).toBe(true);
    expect(items.some((item) => item.header.kind === 'library')).toBe(true);
  });

  it('is offered until a face is installed', () => {
    expect(needsSetup([])).toBe(true);
    expect(needsSetup([{ id: 'timer', name: 'Timer', kind: 'app', enabled: true, bytes: 1, need: null, group: '' }])).toBe(true);
    expect(needsSetup([{ id: 'digital', name: 'Digital', kind: 'face', enabled: true, bytes: 1, need: null, group: '' }])).toBe(false);
  });

  it('installs everything chosen, reporting each step', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const files: FileWriter = {
      write: async (path, data, onProgress) => {
        target.fileService.files.set(path, data);
        onProgress?.({ file: path, sent: data.length, total: data.length });
      },
    };
    const packages = await PackagesClient.open(target);
    const steps: SetupStep[] = [];

    await runSetup(files, packages, setupItems(bundle), (step) => steps.push(step));

    expect((await packages.apps()).map((app) => app.id)).toEqual(['dice', 'digital', 'launcher', 'timer']);
    expect(target.packagesService.library).not.toBeNull();
    expect(needsSetup(await packages.apps())).toBe(false);
    expect(steps.at(-1)).toMatchObject({ index: 4, count: 5 });
    expect(readLauncherFile(target.fileService.files.get(LAUNCHER_PATH)!)?.groups).toEqual(DEFAULT_LAUNCHER.groups);
    expect(target.packagesService.launcherReloads).toBe(1);
  });

  it('offers the library while the step counter waits for one, and installs only the library', async () => {
    const counter = (value: number) => [{ key: 'accelerometerState', label: '', value }];
    expect(needsLibrary(counter(4))).toBe(true);
    expect(needsLibrary(counter(1))).toBe(false);
    expect(needsLibrary([])).toBe(false);

    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
    const files: FileWriter = { write: async (path, data) => void target.fileService.files.set(path, data) };
    const packages = await PackagesClient.open(target);

    await installBundledLibrary(files, packages, setupItems(bundle));

    expect(target.packagesService.library).not.toBeNull();
    expect(await packages.apps()).toEqual([]);
    await expect(installBundledLibrary(files, packages, [])).rejects.toThrow('no shared library');
  });
});
