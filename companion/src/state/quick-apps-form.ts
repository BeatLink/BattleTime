// The Quick apps screen's editor: the watch's ring next to the installed apps that could join it.

import type { PackagesClient, WatchApp } from '@/protocol/packages';
import { readQuickApps, type QuickAppsFiles } from '@/protocol/quick-apps';

// ============================================================================
// Which apps can be in the ring
// ============================================================================

// The launcher is an app on the watch, but stepping into it from the face would only lead back out.
export const NOT_QUICK_APPS: readonly string[] = ['launcher'];

// Whether an installed app may be put in the ring: an app, not a face, and not the launcher.
export function canBeQuickApp(app: WatchApp): boolean {
  return app.kind === 'app' && !NOT_QUICK_APPS.includes(app.id);
}

// The installed apps that may join the ring and are not in it yet, in the launcher's order.
export function quickAppChoices(ring: readonly string[], apps: readonly WatchApp[]): WatchApp[] {
  return apps.filter((app) => canBeQuickApp(app) && !ring.includes(app.id));
}

// ============================================================================
// Changing the ring
// ============================================================================

// The ring with the id added at the end, unless it is already there.
export function addQuickApp(ring: readonly string[], id: string): string[] {
  return ring.includes(id) ? [...ring] : [...ring, id];
}

// The ring without the entry at `index`.
export function removeQuickApp(ring: readonly string[], index: number): string[] {
  return ring.filter((_id, at) => at !== index);
}

// What the screen shows for one entry: the app's name, or its id and a note when it is not installed.
export function describeQuickApp(id: string, apps: readonly WatchApp[]): { name: string; installed: boolean } {
  const app = apps.find((candidate) => candidate.id === id);
  return app ? { name: app.name, installed: true } : { name: id, installed: false };
}

// ============================================================================
// Loading from the watch
// ============================================================================

export type QuickAppsLoaded = { ring: string[]; fromFile: boolean; apps: WatchApp[] };

// Reads the watch's ring and its installed apps, for the screen to open on.
export async function loadQuickApps(
  files: QuickAppsFiles,
  packages: Pick<PackagesClient, 'listing'>,
): Promise<QuickAppsLoaded> {
  const onWatch = await readQuickApps(files);
  const listing = await packages.listing();
  return { ring: onWatch.ids, fromFile: onWatch.fromFile, apps: listing.apps };
}
