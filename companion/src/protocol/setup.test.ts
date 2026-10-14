import { describe, expect, it } from '@jest/globals';

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';
import { BUNDLED_IMAGES } from '@/bundles/watch-apps';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { testImage } from '@/test-support/app-images';

import { readImageHeader } from './app-image';
import { PackagesClient, type FileWriter } from './packages';
import { DEFAULT_LAUNCHER, LAUNCHER_PATH, readLauncherFile } from './launcher';
import { TEMPORARY_ONLY_IDS } from './prompt';
import { needsSetup, runSetup, setupItems, type SetupStep } from './setup';

const bundle = [
  { file: 'timer.ntapp', base64: bytesToBase64(testImage('timer', 'Timer')) },
  { file: 'launcher.ntapp', base64: bytesToBase64(testImage('launcher', 'Launcher')) },
  { file: 'face.ntapp', base64: bytesToBase64(testImage('digital', 'Digital', 'face')) },
  { file: 'shared.ntlib', base64: bytesToBase64(testImage('shared', 'Shared library', 'library')) },
  { file: 'dice.ntapp', base64: bytesToBase64(testImage('dice', 'Dice')) },
  { file: 'prompt.ntapp', base64: bytesToBase64(testImage('prompt', 'Prompt')) },
];

describe('setting up a new watch', () => {
  it('orders the face and the launcher before the apps, requires those two, and leaves the library and temporary apps out', () => {
    const items = setupItems(bundle);
    expect(items.map((item) => item.header.id)).toEqual(['digital', 'launcher', 'dice', 'timer']);
    expect(items.map((item) => item.required)).toEqual([true, true, false, false]);
  });

  it('offers only what the bundle marks for setup', () => {
    const marked = bundle.map((item) => ({ ...item, setup: item.file !== 'dice.ntapp' }));
    expect(setupItems(marked).map((item) => item.header.id)).toEqual(['digital', 'launcher', 'timer']);
  });

  it('reads every image this app bundles for setup except the shared library and the temporary apps', () => {
    const items = setupItems(BUNDLED_IMAGES);
    const forSetup = BUNDLED_IMAGES.filter((item) => item.setup !== false);
    const temporary = forSetup.filter(({ base64 }) => TEMPORARY_ONLY_IDS.has(readImageHeader(base64ToBytes(base64)).id));
    expect(items.length).toBe(forSetup.length - 1 - temporary.length);
    expect(forSetup.length).toBeLessThan(BUNDLED_IMAGES.length);
    expect(items.some((item) => TEMPORARY_ONLY_IDS.has(item.header.id))).toBe(false);
    expect(items.some((item) => item.header.kind === 'face')).toBe(true);
    expect(items.some((item) => item.header.kind === 'library')).toBe(false);
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
    expect(needsSetup(await packages.apps())).toBe(false);
    expect(steps.at(-1)).toMatchObject({ index: 3, count: 4 });
    expect(readLauncherFile(target.fileService.files.get(LAUNCHER_PATH)!)?.groups).toEqual(DEFAULT_LAUNCHER.groups);
    expect(target.packagesService.launcherReloads).toBe(1);
  });
});
