// Save a debug report: the watch's last restart and crash, counters, apps and free storage, as JSON.

import { writeFileSync } from 'node:fs';

import { crashSummary, saveDebugReport } from '@/protocol/debug-report';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const debugUsage = `neotime debug [file]

  debug reads why the watch last restarted and the crash before it, with the
  firmware version, counters, installed apps and free storage, and saves them
  as JSON to the file, or to neotime-debug-<time>.json. Saving it tells the
  watch, which then forgets the crash and leaves its crash screen.`;

export async function debug(session: WatchSession, file?: string): Promise<string> {
  let path = file ?? '';
  const report = await saveDebugReport(
    session,
    {
      apps: async () => (await session.packages()).apps(),
      freeStorageBytes: async () => (await (await session.files()).storage()).free,
    },
    async (fileName, json) => {
      path ||= fileName;
      writeFileSync(path, json);
    },
  );
  say(crashSummary(report.lastRestart));
  say(`Saved the debug report to ${path}.`);
  return path;
}
