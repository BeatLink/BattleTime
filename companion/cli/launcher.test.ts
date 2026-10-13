import { afterEach, describe, expect, it, jest } from '@jest/globals';

import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { FileService } from '@/protocol/file-service';
import { DEFAULT_LAUNCHER, LAUNCHER_PATH, readLauncherFile } from '@/protocol/launcher';
import { installImage, PackagesClient } from '@/protocol/packages';
import { testImage } from '@/test-support/app-images';

import { launcher } from './commands/launcher';

let printed: string[] = [];
const output = () => printed.join('');

afterEach(() => {
  jest.restoreAllMocks();
});

// A mock watch with Snake, which declares games, and Hello, which declares nothing.
async function session() {
  const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'neotime', true);
  const files = await FileService.open(target);
  const packages = await PackagesClient.open(target);
  await installImage(files, packages, testImage('snake', 'Snake', 'app', 0, 'games'));
  await installImage(files, packages, testImage('hello', 'Hello'));
  printed = [];
  jest.spyOn(process.stdout, 'write').mockImplementation((text) => {
    printed.push(String(text));
    return true;
  });
  const onWatch = () => readLauncherFile(target.fileService.files.get(LAUNCHER_PATH)!);
  return { target, onWatch, session: { files: async () => files, packages: async () => packages } };
}

describe('neotime launcher', () => {
  it('shows the default groups on a watch without a file', async () => {
    const { session: watch } = await session();
    await launcher(watch, []);
    expect(output()).toContain('Style folders, apps as a list (the defaults');
    expect(output()).toContain('Games [games, game]: Snake');
    expect(output()).toContain('Other [other, pine_logo]: Hello');
  });

  it('sets the style and the view, and tells the watch', async () => {
    const { session: watch, onWatch, target } = await session();
    await launcher(watch, ['style', 'pages']);
    await launcher(watch, ['view', 'grid']);
    expect(onWatch()).toMatchObject({ style: 'pages', view: 'grid' });
    expect(target.packagesService.launcherReloads).toBe(2);
    await expect(launcher(watch, ['style', 'sideways'])).rejects.toThrow('Give a style');
    await expect(launcher(watch, ['view'])).rejects.toThrow('Give a view');
  });

  it('moves an app, adds a group and removes one', async () => {
    const { session: watch, onWatch } = await session();
    await launcher(watch, ['move', 'hello', 'tools']);
    expect(onWatch()?.placements).toContainEqual({ app: 'hello', group: 'tools' });
    await launcher(watch, ['add', 'Fitness', 'heart']);
    expect(onWatch()?.groups.at(-1)).toEqual({ id: 'fitness', name: 'Fitness', icon: 'heart' });
    await launcher(watch, ['remove', 'tools']);
    expect(onWatch()?.groups.some((group) => group.id === 'tools')).toBe(false);
    await launcher(watch, ['reset']);
    expect(onWatch()?.groups).toEqual(DEFAULT_LAUNCHER.groups);
    expect(onWatch()?.placements).toEqual([
      { app: 'snake', group: 'games' },
      { app: 'hello', group: 'other' },
    ]);
  });

  it('refuses what it cannot do', async () => {
    const { session: watch } = await session();
    await expect(launcher(watch, ['move', 'nope', 'tools'])).rejects.toThrow('installed app');
    await expect(launcher(watch, ['add'])).rejects.toThrow('name');
    await expect(launcher(watch, ['add', 'X', 'teapot'])).rejects.toThrow('icon');
    await expect(launcher(watch, ['remove', 'nope'])).rejects.toThrow('group id');
    await expect(launcher(watch, ['shuffle'])).rejects.toThrow('Unknown launcher action');
  });
});
