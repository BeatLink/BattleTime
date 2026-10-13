// The watch's launcher from the terminal: show its groups, choose its style and view, move an app, or start over.

import { isLibraryIcon } from '@/protocol/remote';
import {
  DEFAULT_LAUNCHER,
  groupApps,
  LAUNCHER_STYLES,
  LAUNCHER_VIEWS,
  newGroupId,
  placeApp,
  readLauncher,
  removeGroup,
  writeLauncher,
  type LauncherSettings,
  type LauncherStyle,
  type LauncherView,
} from '@/protocol/launcher';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const launcherUsage = `neotime launcher [show|style <s>|view <v>|move <app> <group>|add <name> [icon]|remove <group>|reset]

  launcher shows how the watch's launcher groups the apps. style is ${LAUNCHER_STYLES.join(', ')};
  view is ${LAUNCHER_VIEWS.join(' or ')}. move puts an app in a group, add makes a group with a
  shared library icon (pine_logo when left out), remove deletes one, and reset goes back to the
  default groups. Every change is written to /settings/launcher and shown at once.`;

type LauncherSession = Pick<WatchSession, 'files' | 'packages'>;

export async function launcher(session: LauncherSession, args: string[]) {
  const [action = 'show', first, second] = args;
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
      say(`Style ${settings.style}, apps as a ${settings.view}${onWatch ? '' : ' (the defaults; the watch has no launcher file yet)'}.`);
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
    case 'move':
      if (!apps.some((app) => app.id === first) || !settings.groups.some((group) => group.id === second)) {
        throw new Error('Give an installed app and a group id, as show lists them.');
      }
      return save(placeApp(settings, apps, first, second), `${first} is in ${second}.`);
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
