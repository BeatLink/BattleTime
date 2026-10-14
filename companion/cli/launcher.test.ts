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
    expect(output()).toContain('Style folders, apps as a list, sorted by name (the defaults');
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
    expect(onWatch()?.groups).toContainEqual({ id: 'fitness', name: 'Fitness', icon: 'heart' });
    await launcher(watch, ['remove', 'tools']);
    expect(onWatch()?.groups.some((group) => group.id === 'tools')).toBe(false);
    await launcher(watch, ['reset']);
    expect(onWatch()?.groups).toEqual(DEFAULT_LAUNCHER.groups);
    expect(onWatch()?.placements).toEqual([
      { app: 'snake', group: 'games' },
      { app: 'hello', group: 'other' },
    ]);
  });

  it('sorts by name, and switches to custom keeping the order shown', async () => {
    const { session: watch, onWatch } = await session();
    await launcher(watch, ['show']);
    expect(output()).toContain('sorted by name');
    await launcher(watch, ['sort', 'custom']);
    expect(onWatch()?.sort).toBe('custom');
    expect(onWatch()?.groups.map((group) => group.id).slice(0, 3)).toEqual(['communication', 'games', 'health']);
    await launcher(watch, ['sort', 'name']);
    expect(onWatch()?.sort).toBe('name');
    await launcher(watch, ['order', 'time', '1']);
    expect(onWatch()?.sort).toBe('custom');
    expect(onWatch()?.groups[0].id).toBe('time');
    await expect(launcher(watch, ['sort', 'random'])).rejects.toThrow('Give a sort');
  });

  it('refuses what it cannot do', async () => {
    const { session: watch } = await session();
    await expect(launcher(watch, ['move', 'nope', 'tools'])).rejects.toThrow('installed app');
    await expect(launcher(watch, ['add'])).rejects.toThrow('name');
    await expect(launcher(watch, ['add', 'X', 'teapot'])).rejects.toThrow('icon');
    await expect(launcher(watch, ['remove', 'nope'])).rejects.toThrow('group id');
    await expect(launcher(watch, ['shuffle'])).rejects.toThrow('Unknown launcher action');
  });

  it('renames a group, changes its icon and its place, and places an app within a group', async () => {
    const { session: watch, onWatch } = await session();
    await launcher(watch, ['rename', 'games', 'Fun', 'and', 'games']);
    await launcher(watch, ['icon', 'games', 'dice']);
    await launcher(watch, ['order', 'games', '3']);
    expect(onWatch()?.groups[2]).toEqual({ id: 'games', name: 'Fun and games', icon: 'dice' });
    await launcher(watch, ['order', 'games', '99']);
    expect(onWatch()?.groups.at(-1)?.id).toBe('games');

    await launcher(watch, ['move', 'hello', 'games', '1']);
    expect(onWatch()?.placements.filter((placement) => placement.group === 'games').map((placement) => placement.app)).toEqual([
      'hello',
      'snake',
    ]);
    expect(output()).toContain('games is called Fun and games.');
  });

  it('refuses a group, icon or place it cannot use', async () => {
    const { session: watch } = await session();
    await expect(launcher(watch, ['rename', 'nope', 'X'])).rejects.toThrow('group id');
    await expect(launcher(watch, ['rename', 'games'])).rejects.toThrow('new name');
    await expect(launcher(watch, ['icon', 'games', 'teapot'])).rejects.toThrow('shared library icon');
    await expect(launcher(watch, ['order', 'games'])).rejects.toThrow('counting from 1');
    await expect(launcher(watch, ['order', 'games', '0'])).rejects.toThrow('counts from 1');
    await expect(launcher(watch, ['move', 'snake', 'games', 'first'])).rejects.toThrow('counts from 1');
  });
});
