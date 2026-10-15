import { describe, expect, it } from '@jest/globals';

import { LIBRARY_ICON_NAMES } from '@/bundles/icon-names';
import { MockLegacyDfuTarget } from '@/dfu/mock-legacy-target';
import { testImage } from '@/test-support/app-images';

import { readImageHeader } from './app-image';
import { FileService } from './file-service';
import {
  DEFAULT_LAUNCHER,
  followNewGroups,
  groupApps,
  LAUNCHER_PATH,
  LauncherError,
  launcherFile,
  newGroupId,
  OTHER_GROUP_ID,
  readLauncherFile,
  placeApp,
  readLauncher,
  removeGroup,
  sortLauncher,
  writeLauncher,
  type LauncherSettings,
} from './launcher';
import { installImage, PackagesClient, type WatchApp } from './packages';

const icon = (name: string) => LIBRARY_ICON_NAMES.indexOf(name as (typeof LIBRARY_ICON_NAMES)[number]);

const app = (id: string, group = '', extra: Partial<WatchApp> = {}): WatchApp => ({
  id,
  name: id,
  kind: 'app',
  enabled: true,
  bytes: 100,
  need: null,
  group,
  ...extra,
});

const TWO_GROUPS: LauncherSettings = {
  style: 'folders',
  view: 'grid',
  sort: 'custom',
  groups: [
    { id: 'games', name: 'Games', icon: 'game' },
    { id: 'time', name: 'Time apps', icon: 'clock' },
  ],
  placements: [{ app: 'snake', group: 'time' }],
};

const ids = (grouped: ReturnType<typeof groupApps>) => grouped.map(({ group, apps }) => [group.id, apps.map((entry) => entry.id)]);

const text = (value: string) => [value.length, ...new TextEncoder().encode(value)];

describe('the launcher file', () => {
  const installed = [app('alarm', 'time'), app('dice', 'games'), app('snake', 'games'), app('hello')];

  it('holds the style, the view, and each group with its icon, id, name and apps in order', () => {
    expect(Array.from(launcherFile(TWO_GROUPS, installed))).toEqual([
      1, 1, 1, 2,
      icon('game'), ...text('games'), ...text('Games'), 1, ...text('dice'),
      icon('clock'), ...text('time'), ...text('Time apps'), 2, ...text('snake'), ...text('alarm'),
      1,
    ]);
  });

  it('reads back as every app placed where it is', () => {
    const settings = readLauncherFile(launcherFile(TWO_GROUPS, installed));
    expect(settings).toEqual({
      ...TWO_GROUPS,
      placements: [
        { app: 'dice', group: 'games' },
        { app: 'snake', group: 'time' },
        { app: 'alarm', group: 'time' },
      ],
    });
    expect(readLauncherFile(launcherFile(DEFAULT_LAUNCHER, [])) ).toEqual(DEFAULT_LAUNCHER);
  });

  it('starts with time first and system last before other', () => {
    expect(groupApps(DEFAULT_LAUNCHER, []).map(({ group }) => group.id)).toEqual([
      'time',
      'phone',
      'communication',
      'health',
      'games',
      'tools',
      'system',
      'other',
    ]);
  });

  it('keeps the sort in the byte after the groups, and reads a file without it as by name', () => {
    const file = launcherFile({ ...TWO_GROUPS, sort: 'name' }, installed);
    expect(file.at(-1)).toBe(0);
    expect(readLauncherFile(file)?.sort).toBe('name');
    expect(readLauncherFile(launcherFile(TWO_GROUPS, installed).subarray(0, -1))?.sort).toBe('name');
  });

  it('is not a launcher file in another version, and keeps the groups before a cut', () => {
    expect(readLauncherFile(new Uint8Array([2, 1, 1, 0]))).toBeNull();
    expect(readLauncherFile(new Uint8Array(2))).toBeNull();
    const whole = launcherFile(TWO_GROUPS, installed);
    expect(readLauncherFile(whole.subarray(0, whole.length - 3))?.groups.map((group) => group.id)).toEqual(['games', 'time']);
    expect(readLauncherFile(whole.subarray(0, 10))?.groups).toEqual([]);
  });

  it('writes a group without an icon as none, and reads an unknown icon as none', () => {
    const file = launcherFile({ ...TWO_GROUPS, groups: [{ id: 'mine', name: 'Mine', icon: 'none' }] }, []);
    expect(file[4]).toBe(255);
    expect(readLauncherFile(file)?.groups[0].icon).toBe('none');
    file[4] = 250;
    expect(readLauncherFile(file)?.groups[0].icon).toBe('none');
  });

  it('refuses what the watch could not read', () => {
    const tooMany = { ...TWO_GROUPS, groups: Array.from({ length: 25 }, (_, n) => ({ id: `g${n}`, name: 'G', icon: 'game' as const })) };
    expect(() => launcherFile(tooMany, [])).toThrow(LauncherError);
    expect(() => launcherFile({ ...TWO_GROUPS, groups: [{ id: 'my games', name: 'G', icon: 'game' }] }, [])).toThrow('group id');
    expect(() => launcherFile({ ...TWO_GROUPS, groups: [{ id: 'g', name: 'G', icon: 'nope' as 'game' }] }, [])).toThrow('no icon');
  });

  it('writes a name on one line, its id when it is blank, and cuts a long one to 255 bytes', () => {
    const named = (name: string) => readLauncherFile(launcherFile({ ...TWO_GROUPS, groups: [{ id: 'g', name, icon: 'game' }] }, []))?.groups[0].name;
    expect(named('Fun\nand games ')).toBe('Fun and games');
    expect(named('  ')).toBe('g');
    expect(new TextEncoder().encode(named('é'.repeat(200))).length).toBe(254);
  });
});

describe('grouping the apps', () => {
  const apps = [app('alarm', 'time'), app('dice', 'games'), app('snake', 'games'), app('hello'), app('launcher', 'system')];

  it('puts each app in the group it declares, a placed one where it was put, and the rest in other', () => {
    expect(ids(groupApps(TWO_GROUPS, apps))).toEqual([
      ['games', ['dice']],
      ['time', ['snake', 'alarm']],
      ['other', ['hello']],
    ]);
  });

  it('sorts groups and apps by name, other last, ignoring placements', () => {
    const named = [app('zen', 'games', { name: 'Abacus' }), app('dice', 'games', { name: 'dice' }), app('beat', 'games', { name: 'Chess' }), app('hello')];
    const groups = [{ id: OTHER_GROUP_ID, name: 'Aaa', icon: 'pine_logo' as const }, ...[...TWO_GROUPS.groups].reverse()];
    const settings: LauncherSettings = { ...TWO_GROUPS, sort: 'name', groups, placements: [{ app: 'dice', group: 'games' }] };
    expect(ids(groupApps(settings, named))).toEqual([
      ['games', ['zen', 'beat', 'dice']],
      ['time', []],
      ['other', ['hello']],
    ]);
    expect(readLauncherFile(launcherFile(settings, named))?.groups.map((group) => group.id)).toEqual(['games', 'time', OTHER_GROUP_ID]);
  });

  it('puts apps the wearer has not ordered after the ordered ones, by name', () => {
    const named = [app('zed', 'games'), app('apple', 'games'), app('snake', 'games')];
    expect(ids(groupApps({ ...TWO_GROUPS, placements: [{ app: 'snake', group: 'games' }] }, named))[0]).toEqual(['games', ['snake', 'apple', 'zed']]);
  });

  it('goes to custom keeping the order shown', () => {
    const custom = sortLauncher({ ...TWO_GROUPS, sort: 'name', groups: [...TWO_GROUPS.groups].reverse() }, apps, 'custom');
    expect(custom.groups.map((group) => group.id)).toEqual(['games', 'time']);
    expect(custom.placements).toEqual([
      { app: 'dice', group: 'games' },
      { app: 'alarm', group: 'time' },
      { app: 'snake', group: 'time' },
      { app: 'hello', group: 'other' },
    ]);
    expect(sortLauncher(custom, apps, 'name')).toEqual({ ...custom, sort: 'name' });
  });

  it('leaves out faces and the launcher, but keeps hidden apps in their place', () => {
    const grouped = groupApps(DEFAULT_LAUNCHER, [app('digital', '', { kind: 'face' }), app('dice', 'games', { enabled: false }), app('launcher')]);
    expect(grouped.flatMap((entry) => entry.apps.map((member) => member.id))).toEqual(['dice']);
  });

  it('moves an app into a group at a place, and keeps the order written', () => {
    const moved = placeApp(TWO_GROUPS, apps, 'alarm', 'games', 0);
    expect(ids(groupApps(moved, apps)).slice(0, 2)).toEqual([
      ['games', ['alarm', 'dice']],
      ['time', ['snake']],
    ]);
    const last = placeApp(moved, apps, 'snake', 'games');
    expect(ids(groupApps(last, apps))[0]).toEqual(['games', ['alarm', 'dice', 'snake']]);
  });

  it('sends a removed group\'s apps back to where they belong', () => {
    const settings = removeGroup(TWO_GROUPS, 'time');
    expect(settings.groups.map((group) => group.id)).toEqual(['games']);
    expect(ids(groupApps(settings, apps))).toEqual([
      ['games', ['dice', 'snake']],
      ['other', ['alarm', 'hello']],
    ]);
  });

  it('makes a plain id for a new group that no other group has', () => {
    expect(newGroupId(TWO_GROUPS, 'My Games!')).toBe('my-games');
    expect(newGroupId(TWO_GROUPS, 'Games')).toBe('games-2');
    expect(newGroupId(TWO_GROUPS, '!!!')).toBe('group');
  });
});

describe('the launcher on the watch', () => {
  it('starts from the defaults on a watch without a file, then reads back what was written', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
    const files = await FileService.open(target);
    const packages = await PackagesClient.open(target);

    expect(await readLauncher(files)).toEqual({ settings: DEFAULT_LAUNCHER, onWatch: false });

    await writeLauncher(files, packages, TWO_GROUPS);

    expect(target.fileService.files.get(LAUNCHER_PATH)).toEqual(launcherFile(TWO_GROUPS, []));
    expect(target.packagesService.launcherReloads).toBe(1);
    expect(await readLauncher(files)).toEqual({ settings: { ...TWO_GROUPS, placements: [] }, onWatch: true });
    packages.close();
  });

  it('lists each installed app with the group its image declares', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
    const files = await FileService.open(target);
    const packages = await PackagesClient.open(target);

    await installImage(files, packages, testImage('snake', 'Snake', 'app', 0, 'games'));
    await installImage(files, packages, testImage('timer', 'Timer'));

    expect((await packages.apps()).map((entry) => [entry.id, entry.group])).toEqual([
      ['snake', 'games'],
      ['timer', ''],
    ]);
    expect(readImageHeader(testImage('snake', 'Snake', 'app', 0, 'games')).group).toBe('games');
    packages.close();
  });

  it('places an app installed later in the group its image declares, once the watch has groups', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
    const files = await FileService.open(target);
    const packages = await PackagesClient.open(target);
    await installImage(files, packages, testImage('dice', 'Dice', 'app', 0, 'games'));
    expect(target.fileService.files.has(LAUNCHER_PATH)).toBe(false);

    await writeLauncher(files, packages, DEFAULT_LAUNCHER);
    await installImage(files, packages, testImage('snake', 'Snake', 'app', 0, 'games'));

    const { settings } = await readLauncher(files);
    expect(settings.placements.filter((placement) => placement.group === 'games').map((placement) => placement.app)).toEqual(['dice', 'snake']);
    expect(target.packagesService.launcherReloads).toBe(2);
    packages.close();
  });

  it('moves a reinstalled app to the group its new image declares, unless the wearer moved it', async () => {
    const target = new MockLegacyDfuTarget('AA:BB:CC:DD:EE:00', 'battletime', true);
    const files = await FileService.open(target);
    const packages = await PackagesClient.open(target);
    await installImage(files, packages, testImage('sports', 'Sports', 'app', 0, 'time'));
    await installImage(files, packages, testImage('dice', 'Dice', 'app', 0, 'games'));
    await writeLauncher(files, packages, DEFAULT_LAUNCHER);
    const { settings: placed } = await readLauncher(files);
    await writeLauncher(files, packages, placeApp(placed, await packages.apps(), 'dice', 'tools'));

    await installImage(files, packages, testImage('sports', 'Sports', 'app', 0, 'health'));
    await installImage(files, packages, testImage('dice', 'Dice', 'app', 0, 'phone'));

    const { settings } = await readLauncher(files);
    const shown = groupApps(settings, await packages.apps()).filter(({ apps }) => apps.length > 0);
    expect(shown.map(({ group, apps }) => [group.id, apps.map((app) => app.id)])).toEqual([
      ['health', ['sports']],
      ['tools', ['dice']],
    ]);
    packages.close();
  });

  it('lets only an app still in its old declared group follow its new one', () => {
    const placements = [
      { app: 'sports', group: 'time' },
      { app: 'dice', group: 'tools' },
    ];
    const moves = [
      { app: 'sports', from: 'time' },
      { app: 'dice', from: 'games' },
    ];

    expect(followNewGroups({ ...DEFAULT_LAUNCHER, placements }, moves).placements).toEqual([{ app: 'dice', group: 'tools' }]);
  });
});
