// The guides the watch's survival app lists: each a text file at /survival/<slot>, and their topics and titles in
// /survival/list.
//
// docs/companion-protocol.md in the BattleTime repository describes the files. They travel over the file
// service; sending a choice of guides writes only the guides the watch lacks or holds at another length, then the
// list, then removes the guides left out.

import { SURVIVAL_GUIDES, SURVIVAL_TOPICS, guideInSlot, type SurvivalGuide } from '@/bundles/survival-guides';
import { FileMissingError, type FileService } from '@/protocol/file-service';

// ============================================================================
// The files
// ============================================================================

export const SURVIVAL_DIRECTORY = '/survival';
export const SURVIVAL_LIST_PATH = `${SURVIVAL_DIRECTORY}/list`;
export const SURVIVAL_LIST_VERSION = 2;

// The most guides the watch keeps, each in its own slot from 0.
export const MOST_SURVIVAL_GUIDES = 24;

// The longest title the watch lists.
export const LARGEST_SURVIVAL_TITLE_BYTES = 23;

const ENTRY_BYTES = 3 + LARGEST_SURVIVAL_TITLE_BYTES;

export class SurvivalError extends Error {}

// The file holding the guide in a slot.
export function survivalGuidePath(slot: number): string {
  return `${SURVIVAL_DIRECTORY}/${slot}`;
}

export function guideBytes(guide: SurvivalGuide): Uint8Array {
  return new TextEncoder().encode(guide.text);
}

// The number the list file gives a guide's topic.
export function topicNumber(guide: SurvivalGuide): number {
  return SURVIVAL_TOPICS.findIndex((topic) => topic.id === guide.topic);
}

// ============================================================================
// The list file
// ============================================================================

// A guide as the watch lists it.
export type SurvivalEntry = { slot: number; topic: number; title: string };

// A /survival/list file naming these guides, in order: a version, a count, then per guide its slot, its topic's
// number, its title's length and the title padded with zeros.
export function packSurvivalList(entries: SurvivalEntry[]): Uint8Array {
  if (entries.length > MOST_SURVIVAL_GUIDES) {
    throw new SurvivalError(`The watch keeps up to ${MOST_SURVIVAL_GUIDES} guides.`);
  }
  const file = new Uint8Array(2 + entries.length * ENTRY_BYTES);
  file[0] = SURVIVAL_LIST_VERSION;
  file[1] = entries.length;
  entries.forEach((entry, place) => {
    const title = new TextEncoder().encode(entry.title);
    if (
      title.length > LARGEST_SURVIVAL_TITLE_BYTES ||
      entry.slot >= MOST_SURVIVAL_GUIDES ||
      entry.topic >= SURVIVAL_TOPICS.length
    ) {
      throw new SurvivalError(`The guide "${entry.title}" does not fit the watch's list.`);
    }
    const start = 2 + place * ENTRY_BYTES;
    file[start] = entry.slot;
    file[start + 1] = entry.topic;
    file[start + 2] = title.length;
    file.set(title, start + 3);
  });
  return file;
}

// The entries of a /survival/list file, or none for a file in another format.
export function parseSurvivalList(file: Uint8Array): SurvivalEntry[] {
  if (file[0] !== SURVIVAL_LIST_VERSION) {
    return [];
  }
  const count = Math.min(file[1] ?? 0, MOST_SURVIVAL_GUIDES);
  const entries: SurvivalEntry[] = [];
  for (let place = 0; place < count; place++) {
    const start = 2 + place * ENTRY_BYTES;
    if (start + ENTRY_BYTES > file.length) {
      break;
    }
    const length = Math.min(file[start + 2], LARGEST_SURVIVAL_TITLE_BYTES);
    const title = new TextDecoder().decode(file.subarray(start + 3, start + 3 + length));
    entries.push({ slot: file[start], topic: file[start + 1], title });
  }
  return entries;
}

// ============================================================================
// Changing the guides on the watch
// ============================================================================

export type SurvivalFiles = Pick<FileService, 'read' | 'write' | 'remove' | 'list'>;

async function removeIfThere(files: SurvivalFiles, path: string): Promise<void> {
  try {
    await files.remove(path);
  } catch (error) {
    if (!(error instanceof FileMissingError)) {
      throw error;
    }
  }
}

// The guides on the watch in the order it lists them, or none when it has no list.
export async function readSurvivalList(files: SurvivalFiles): Promise<SurvivalEntry[]> {
  try {
    return parseSurvivalList(await files.read(SURVIVAL_LIST_PATH));
  } catch (error) {
    if (error instanceof FileMissingError) {
      return [];
    }
    throw error;
  }
}

// The ids of the guides the watch lists, skipping any slot this app has no guide for.
export function guideIdsIn(entries: SurvivalEntry[]): string[] {
  return entries.flatMap((entry) => guideInSlot(entry.slot)?.id ?? []);
}

// The sizes of the files in /survival on the watch, or none when the folder is missing.
async function sizesOnWatch(files: SurvivalFiles): Promise<Map<string, number>> {
  try {
    const entries = await files.list(SURVIVAL_DIRECTORY);
    return new Map(entries.filter((entry) => !entry.directory).map((entry) => [entry.path, entry.size]));
  } catch {
    return new Map();
  }
}

export type SurvivalSendResult = {
  entries: SurvivalEntry[];
  sent: number;
  removed: number;
};

// Puts exactly the guides with these ids on the watch, in the app's own order; gives the new list.
//
// A guide already on the watch at the same length is not sent again. With no ids, every guide comes off and the
// list goes, so the watch app shows its hint again.
export async function sendSurvivalGuides(files: SurvivalFiles, ids: readonly string[]): Promise<SurvivalSendResult> {
  const unknown = ids.filter((id) => !SURVIVAL_GUIDES.some((guide) => guide.id === id));
  if (unknown.length > 0) {
    throw new SurvivalError(`There is no guide called ${unknown.join(', ')}.`);
  }
  const chosen = SURVIVAL_GUIDES.filter((guide) => ids.includes(guide.id));
  const before = await readSurvivalList(files);
  const sizes = await sizesOnWatch(files);

  // The guides go first, so the list never names a slot whose file is not there yet.
  let sent = 0;
  for (const guide of chosen) {
    const bytes = guideBytes(guide);
    if (sizes.get(survivalGuidePath(guide.slot)) !== bytes.length) {
      await files.write(survivalGuidePath(guide.slot), bytes);
      sent++;
    }
  }

  const entries = chosen.map((guide) => ({ slot: guide.slot, topic: topicNumber(guide), title: guide.title }));
  if (entries.length > 0) {
    await files.write(SURVIVAL_LIST_PATH, packSurvivalList(entries));
  } else {
    await removeIfThere(files, SURVIVAL_LIST_PATH);
  }

  // The list goes before the guides it drops, so it never names a slot whose file is gone.
  const kept = new Set(entries.map((entry) => entry.slot));
  const stale = new Set([
    ...before.map((entry) => entry.slot),
    ...[...sizes.keys()].flatMap((path) => {
      const name = path.slice(SURVIVAL_DIRECTORY.length + 1);
      return /^\d+$/.test(name) ? [Number(name)] : [];
    }),
  ]);
  let removed = 0;
  for (const slot of stale) {
    if (!kept.has(slot)) {
      await removeIfThere(files, survivalGuidePath(slot));
      removed++;
    }
  }
  return { entries, sent, removed };
}
