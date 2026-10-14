// The watch's launcher from the terminal: everything the Launcher screen does, from choosing the style to editing groups.

import { isLibraryIcon } from '@/protocol/remote';
import {
  DEFAULT_LAUNCHER,
  groupApps,
  LAUNCHER_SORTS,
  LAUNCHER_STYLES,
  LAUNCHER_VIEWS,
  newGroupId,
  placeApp,
  readLauncher,
  removeGroup,
  sortLauncher,
  writeLauncher,
  type LauncherSettings,
  type LauncherSort,
  type LauncherStyle,
  type LauncherView,
} from '@/protocol/launcher';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const launcherUsage = `neotime launcher [show|style <s>|view <v>|sort <name|custom>|move <app> <group> [place]|add <name> [icon]|
                  rename <group> <name>|icon <group> <icon>|order <group> <place>|remove <group>|reset]

  launcher shows how the watch's launcher groups the apps. style is ${LAUNCHER_STYLES.join(', ')};
  view is ${LAUNCHER_VIEWS.join(' or ')}. sort name puts groups and apps in order of their names; sort
  custom keeps the order shown, and ordering a group or placing an app at a place sorts custom. move puts an app in a group, at a place counting from 1 or
  last; add makes a group with a shared library icon (pine_logo when left out); rename, icon and
  order change a group's name, icon and place among the groups; remove deletes one, and reset goes
  back to the default groups. Every change is written to /settings/launcher and shown at once.`;

type LauncherSession = Pick<WatchSession, 'files' | 'packages'>;

export async function launcher(session: LauncherSession, args: string[]) {
  const [action = 'show', first, second, ...rest] = args;
  const files = await session.files();
  const packages = await session.packages();
  const { settings, onWatch } = await readLauncher(files);
  const apps = await packages.apps();
  const save = async (next: LauncherSettings, what: string) => {
    await writeLauncher(files, packages, next);
    say(what);
  };
  switch (action) {
    case 'show':
      say(`Style ${settings.style}, apps as a ${settings.view}, sorted ${settings.sort === 'name' ? 'by name' : 'as placed'}${onWatch ? '' : ' (the defaults; the watch has no launcher file yet)'}.`);
      for (const { group, apps: members } of groupApps(settings, apps)) {
        say(`${group.name} [${group.id}, ${group.icon}]: ${members.map((member) => member.name).join(', ') || '-'}`);
      }
      return;
    case 'style':
      if (!(LAUNCHER_STYLES as readonly string[]).includes(first)) {
        throw new Error(`Give a style: ${LAUNCHER_STYLES.join(', ')}.`);
      }
      return save({ ...settings, style: first as LauncherStyle }, `The launcher shows ${first}.`);
    case 'view':
      if (!(LAUNCHER_VIEWS as readonly string[]).includes(first)) {
        throw new Error(`Give a view: ${LAUNCHER_VIEWS.join(' or ')}.`);
      }
      return save({ ...settings, view: first as LauncherView }, `The launcher shows apps as a ${first}.`);
    case 'sort':
      if (!(LAUNCHER_SORTS as readonly string[]).includes(first)) {
        throw new Error(`Give a sort: ${LAUNCHER_SORTS.join(' or ')}.`);
      }
      return save(sortLauncher(settings, apps, first as LauncherSort), `The launcher sorts ${first === 'name' ? 'by name' : 'as placed'}.`);
    case 'move': {
      if (!apps.some((app) => app.id === first) || !settings.groups.some((group) => group.id === second)) {
        throw new Error('Give an installed app and a group id, as show lists them.');
      }
      if (rest[0] === undefined) {
        return save(placeApp(settings, apps, first, second), `${first} is in ${second}.`);
      }
      const place = placeNumber(rest[0]);
      return save(placeApp(customised(settings, apps), apps, first, second, place), `${first} is in ${second}.`);
    }
    case 'rename': {
      const name = [second, ...rest].filter(Boolean).join(' ').trim();
      if (!hasGroup(settings, first) || !name) {
        throw new Error('Give a group id, as show lists them, and its new name.');
      }
      return save(editGroup(settings, first, { name }), `${first} is called ${name}.`);
    }
    case 'icon':
      if (!hasGroup(settings, first) || !isLibraryIcon(second)) {
        throw new Error('Give a group id, as show lists them, and a shared library icon.');
      }
      return save(editGroup(settings, first, { icon: second }), `${first} shows ${second}.`);
    case 'order': {
      if (!hasGroup(settings, first) || second === undefined) {
        throw new Error('Give a group id, as show lists them, and its place counting from 1.');
      }
      const custom = customised(settings, apps);
      const from = custom.groups.findIndex((group) => group.id === first);
      const to = Math.min(placeNumber(second), custom.groups.length - 1);
      const groups = custom.groups.filter((group) => group.id !== first);
      groups.splice(to, 0, custom.groups[from]);
      return save({ ...custom, groups }, `${first} is group ${to + 1}.`);
    }
    case 'add': {
      const icon = second ?? 'pine_logo';
      if (!first || !isLibraryIcon(icon)) {
        throw new Error('Give the new group a name, and a shared library icon if you like.');
      }
      const id = newGroupId(settings, first);
      return save({ ...settings, groups: [...settings.groups, { id, name: first, icon }] }, `Added ${first} as ${id}.`);
    }
    case 'remove':
      if (!settings.groups.some((group) => group.id === first)) {
        throw new Error('Give a group id, as show lists them.');
      }
      return save(removeGroup(settings, first), `Removed ${first}.`);
    case 'reset':
      return save(DEFAULT_LAUNCHER, 'The launcher has the default groups again.');
    default:
      throw new Error(`Unknown launcher action: ${action}`);
  }
}

// The settings sorted custom, keeping the order shown, so the wearer's own order can be changed.
function customised(settings: LauncherSettings, apps: Parameters<typeof sortLauncher>[1]): LauncherSettings {
  return settings.sort === 'custom' ? settings : sortLauncher(settings, apps, 'custom');
}

function hasGroup(settings: LauncherSettings, id: string | undefined): boolean {
  return settings.groups.some((group) => group.id === id);
}

function editGroup(settings: LauncherSettings, id: string, patch: Partial<LauncherSettings['groups'][number]>): LauncherSettings {
  return { ...settings, groups: settings.groups.map((group) => (group.id === id ? { ...group, ...patch } : group)) };
}

// A place counting from 1, as people number them, turned to an index from 0.
function placeNumber(text: string): number {
  const place = Number(text);
  if (!Number.isInteger(place) || place < 1) {
    throw new Error(`A place counts from 1, not ${text}.`);
  }
  return place - 1;
}
