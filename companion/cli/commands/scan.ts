// List the watches in range.

import type { WatchSession } from '../ble';
import { say } from '../output';
import { remember } from '../target';

export const scanUsage = `waspos scan [--timeout <seconds>]

  Lists the watches in range. A single watch is remembered, so later
  commands need no address.`;

export async function scan(session: WatchSession, timeoutMs: number): Promise<void> {
  const found = await session.scan(timeoutMs);
  if (found.length === 0) {
    say('No watch found.');
    return;
  }
  for (const watch of found) {
    const rssi = watch.rssi === null ? '' : `  ${watch.rssi} dBm`;
    say(`${watch.id}  ${watch.name}${rssi}`);
  }
  if (found.length === 1) {
    remember(found[0].id, found[0].name);
  }
}
