// The watch's survival app from the terminal: list the emergency guides and which the watch carries, and choose
// the ones it carries.

import { SURVIVAL_GUIDES, SURVIVAL_TOPICS } from '@/bundles/survival-guides';
import { guideIdsIn, readSurvivalList, sendSurvivalGuides } from '@/protocol/survival';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const survivalUsage = `battletime survival [list]
battletime survival set <guide>...|all
battletime survival add <guide>...
battletime survival remove <guide>...|all

  survival lists every emergency guide by its name under its topic, ticking the
  ones the watch's survival app carries. set puts exactly the guides named on the watch, add and
  remove change the ones it has, and all stands for every guide.`;

// ============================================================================
// Arguments
// ============================================================================

export type SurvivalRequest = { kind: 'list' } | { kind: 'set' | 'add' | 'remove'; ids: string[] };

const EVERY_ID = SURVIVAL_GUIDES.map((guide) => guide.id);

// Reads the command's arguments into a listing or a change to the guides on the watch.
export function parseSurvivalArguments(args: string[]): SurvivalRequest {
  const [first, ...names] = args;
  if (first === undefined || (first === 'list' && names.length === 0)) {
    return { kind: 'list' };
  }
  if (first !== 'set' && first !== 'add' && first !== 'remove') {
    throw new Error('Give list, set, add or remove.');
  }
  if (names.length === 0) {
    throw new Error(`Name the guides to ${first}: ${EVERY_ID.join(', ')}, or all.`);
  }
  if (names.includes('all')) {
    return { kind: first, ids: EVERY_ID };
  }
  const unknown = names.filter((name) => !EVERY_ID.includes(name));
  if (unknown.length > 0) {
    throw new Error(`There is no guide called ${unknown.join(', ')}; the guides are ${EVERY_ID.join(', ')}.`);
  }
  return { kind: first, ids: names };
}

// ============================================================================
// The command
// ============================================================================

type SurvivalSession = Pick<WatchSession, 'files'>;

// Prints every guide under its topic, ticked when it is on the watch.
function sayGuides(onWatch: string[]) {
  for (const topic of SURVIVAL_TOPICS) {
    say(topic.title);
    for (const guide of SURVIVAL_GUIDES.filter((each) => each.topic === topic.id)) {
      say(`  [${onWatch.includes(guide.id) ? 'x' : ' '}] ${guide.id.padEnd(20)} ${guide.title}`);
    }
  }
}

export async function survival(session: SurvivalSession, args: string[]) {
  const request = parseSurvivalArguments(args);
  const files = await session.files();
  const onWatch = guideIdsIn(await readSurvivalList(files));
  if (request.kind === 'list') {
    sayGuides(onWatch);
    return;
  }
  const wanted =
    request.kind === 'set'
      ? request.ids
      : request.kind === 'add'
        ? [...onWatch, ...request.ids]
        : onWatch.filter((id) => !request.ids.includes(id));
  const result = await sendSurvivalGuides(files, wanted);
  sayGuides(guideIdsIn(result.entries));
  say(`Sent ${result.sent} guide${result.sent === 1 ? '' : 's'} and removed ${result.removed}.`);
}
