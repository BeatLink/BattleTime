// How the watch's launcher groups the apps: its style, its view and the groups, kept in /settings/launcher.
//
// The file says where every app goes, as services/packages/src/launcher.rs in the NeoTime tree reads it:
// byte 0 the format version (1), byte 1 the style, byte 2 the view, byte 3 the group count, then per group
// its icon's place in the shared library, its id, its name, and its apps' ids in order, each text behind a
// length byte. This app keeps the wearer's choices as settings and works out the file from them and the
// installed apps, each placed in the group its image declares until the wearer moves it. After writing
// the file, the `launcher` packages command makes the watch show it.

import { LIBRARY_ICON_NAMES, type LibraryIconName } from '@/bundles/icon-names';
import type { FileService } from '@/protocol/file-service';
import type { WatchApp } from '@/protocol/packages';

// ============================================================================
// Types
// ============================================================================

export const LAUNCHER_PATH = '/settings/launcher';

// The group an app goes to when the group it names is missing.
export const OTHER_GROUP_ID = 'other';

// The most groups the watch keeps.
export const MOST_GROUPS = 24;

export const LAUNCHER_STYLES = ['folders', 'sections', 'pages', 'list'] as const;
export type LauncherStyle = (typeof LAUNCHER_STYLES)[number];

export const LAUNCHER_VIEWS = ['list', 'grid'] as const;
export type LauncherView = (typeof LAUNCHER_VIEWS)[number];

export type LauncherGroup = { id: string; name: string; icon: LibraryIconName };

// One app placed in a group by the wearer; the list's order is the apps' order within each group.
export type Placement = { app: string; group: string };

export type LauncherSettings = {
  style: LauncherStyle;
  view: LauncherView;
  groups: LauncherGroup[];
  placements: Placement[];
};

// The groups a watch starts with, which the usual apps declare.
export const DEFAULT_GROUPS: LauncherGroup[] = [
  { id: 'games', name: 'Games', icon: 'game' },
  { id: 'tools', name: 'Tools', icon: 'calculator' },
  { id: 'communication', name: 'Communication', icon: 'notification' },
  { id: 'time', name: 'Time', icon: 'clock' },
  { id: 'phone', name: 'Phone', icon: 'phone' },
  { id: 'health', name: 'Health', icon: 'heart' },
  { id: 'system', name: 'System', icon: 'settings' },
  { id: OTHER_GROUP_ID, name: 'Other', icon: 'pine_logo' },
];

export const DEFAULT_LAUNCHER: LauncherSettings = {
  style: 'folders',
  view: 'list',
  groups: DEFAULT_GROUPS,
  placements: [],
};

export class LauncherError extends Error {}

const isPlainId = (id: string) => /^[A-Za-z0-9_-]{1,16}$/.test(id);

// ============================================================================
// The file
// ============================================================================

export const LAUNCHER_FILE_VERSION = 1;

// An icon's value for none, which shows the group's first app's icon.
const NO_ICON = 255;

const MOST_TEXT_BYTES = 255;

const STYLE_NUMBERS: Record<LauncherStyle, number> = { list: 0, folders: 1, sections: 2, pages: 3 };
const VIEW_NUMBERS: Record<LauncherView, number> = { list: 0, grid: 1 };

// A length byte and the text, cut to whole characters within 255 bytes.
function textBytes(text: string): number[] {
  let bytes = new TextEncoder().encode(text);
  for (let cut = text.length; bytes.length > MOST_TEXT_BYTES; cut -= 1) {
    bytes = new TextEncoder().encode(text.slice(0, cut));
  }
  return [bytes.length, ...bytes];
}

// The file for the watch: the settings with every installed app placed; throws when a group would not survive the watch's reading.
export function launcherFile(settings: LauncherSettings, apps: readonly WatchApp[]): Uint8Array {
  if (settings.groups.length > MOST_GROUPS) {
    throw new LauncherError(`The watch keeps up to ${MOST_GROUPS} groups, not ${settings.groups.length}.`);
  }
  const bytes = [LAUNCHER_FILE_VERSION, STYLE_NUMBERS[settings.style], VIEW_NUMBERS[settings.view], settings.groups.length];
  const grouped = new Map(groupApps(settings, apps).map(({ group, apps: members }) => [group.id, members]));
  for (const group of settings.groups) {
    if (!isPlainId(group.id)) {
      throw new LauncherError(`A group id is 1 to 16 letters, digits, "-" and "_", not "${group.id}".`);
    }
    const icon = (LIBRARY_ICON_NAMES as readonly string[]).indexOf(group.icon);
    if (icon < 0) {
      throw new LauncherError(`The shared library has no icon called ${group.icon}.`);
    }
    const members = grouped.get(group.id) ?? [];
    const name = group.name.replace(/[\r\n]+/g, ' ').trim() || group.id;
    bytes.push(group.icon === 'none' ? NO_ICON : icon, ...textBytes(group.id), ...textBytes(name), members.length);
    for (const member of members) {
      bytes.push(...textBytes(member.id));
    }
  }
  return new Uint8Array(bytes);
}

// Reads a launcher file back, every app in it placed where it is; null when it is not one.
export function readLauncherFile(file: Uint8Array): LauncherSettings | null {
  if (file.length < 4 || file[0] !== LAUNCHER_FILE_VERSION) {
    return null;
  }
  let at = 4;
  const text = () => {
    const length = file[at];
    const bytes = file.subarray(at + 1, at + 1 + length);
    if (length === undefined || bytes.length !== length) throw new LauncherError('The launcher file is cut short.');
    at += 1 + length;
    return new TextDecoder().decode(bytes);
  };
  const settings: LauncherSettings = {
    style: LAUNCHER_STYLES.find((style) => STYLE_NUMBERS[style] === file[1]) ?? 'list',
    view: LAUNCHER_VIEWS.find((view) => VIEW_NUMBERS[view] === file[2]) ?? 'list',
    groups: [],
    placements: [],
  };
  try {
    for (let group = 0; group < file[3]; group++) {
      const iconNumber = file[at];
      at += 1;
      const id = text();
      const name = text();
      const count = file[at];
      at += 1;
      if (count === undefined) throw new LauncherError('The launcher file is cut short.');
      const icon = iconNumber === NO_ICON ? 'none' : (LIBRARY_ICON_NAMES[iconNumber] ?? 'none');
      settings.groups.push({ id, name, icon });
      for (let app = 0; app < count; app++) {
        settings.placements.push({ app: text(), group: id });
      }
    }
  } catch {
    // The groups before the cut stay, as on the watch.
  }
  return settings;
}

// ============================================================================
// Grouping, as the watch does
// ============================================================================

// Each group with the apps the launcher shows in it, in order; `other` is added when apps land there.
export function groupApps(settings: LauncherSettings, apps: readonly WatchApp[]): { group: LauncherGroup; apps: WatchApp[] }[] {
  // Hidden apps keep their place too, so turning one back on puts it where it was.
  const listed = apps.filter((app) => app.kind === 'app' && app.id !== 'launcher');
  const groups = [...settings.groups];
  if (!groups.some((group) => group.id === OTHER_GROUP_ID)) {
    groups.push(DEFAULT_GROUPS.find((group) => group.id === OTHER_GROUP_ID)!);
  }
  const ids = new Set(groups.map((group) => group.id));
  const groupOf = (app: WatchApp) => {
    const wanted = settings.placements.find((placement) => placement.app === app.id)?.group ?? app.group;
    return wanted && ids.has(wanted) ? wanted : OTHER_GROUP_ID;
  };
  const rank = (app: WatchApp) => {
    const placed = settings.placements.findIndex((placement) => placement.app === app.id);
    return placed >= 0 ? placed : settings.placements.length + listed.indexOf(app);
  };
  return groups.map((group) => ({
    group,
    apps: listed.filter((app) => groupOf(app) === group.id).sort((a, b) => rank(a) - rank(b)),
  }));
}

// Moves an app into a group, last there, or to `place` among that group's apps.
export function placeApp(settings: LauncherSettings, apps: readonly WatchApp[], app: string, group: string, place?: number): LauncherSettings {
  // Every app of the group gets a placement, so the order the wearer sees is the order written.
  const grouped = groupApps(settings, apps);
  const members = (grouped.find((entry) => entry.group.id === group)?.apps ?? []).map((member) => member.id).filter((id) => id !== app);
  members.splice(place === undefined ? members.length : Math.max(0, Math.min(place, members.length)), 0, app);
  const others = settings.placements.filter((placement) => placement.group !== group && placement.app !== app && !members.includes(placement.app));
  return { ...settings, placements: [...others, ...members.map((id) => ({ app: id, group }))] };
}

// Removes a group; its apps go back to the groups they declare, or to `other`.
export function removeGroup(settings: LauncherSettings, id: string): LauncherSettings {
  return {
    ...settings,
    groups: settings.groups.filter((group) => group.id !== id),
    placements: settings.placements.filter((placement) => placement.group !== id),
  };
}

// A new group's id from its name, made unique among the groups there are.
export function newGroupId(settings: LauncherSettings, name: string): string {
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 12) || 'group';
  let id = base;
  for (let number = 2; settings.groups.some((group) => group.id === id); number += 1) {
    id = `${base}-${number}`;
  }
  return id;
}

// ============================================================================
// On the watch
// ============================================================================

export type LauncherFiles = Pick<FileService, 'read' | 'write'>;

// The watch's launcher settings, or the defaults when it holds none.
export async function readLauncher(files: LauncherFiles): Promise<{ settings: LauncherSettings; onWatch: boolean }> {
  try {
    const parsed = readLauncherFile(await files.read(LAUNCHER_PATH));
    if (parsed) {
      return { settings: parsed, onWatch: true };
    }
  } catch {
    // No file yet.
  }
  return { settings: DEFAULT_LAUNCHER, onWatch: false };
}

// Writes the settings with every installed app placed, and has the launcher show them.
export async function writeLauncher(
  files: { write(path: string, data: Uint8Array): Promise<void> },
  packages: { apps(): Promise<WatchApp[]>; reloadLauncher(): Promise<void> },
  settings: LauncherSettings,
): Promise<void> {
  await files.write(LAUNCHER_PATH, launcherFile(settings, await packages.apps()));
  await packages.reloadLauncher();
}

// After an install, places the new app where its image says, when the watch has launcher groups.
export async function placeNewApps(files: LauncherFiles, packages: { apps(): Promise<WatchApp[]>; reloadLauncher(): Promise<void> }) {
  const { settings, onWatch } = await readLauncher(files);
  if (onWatch) {
    await writeLauncher(files, packages, settings);
  }
}
