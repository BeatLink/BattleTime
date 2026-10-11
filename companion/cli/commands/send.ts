// Send one message over the UART channel, and read NeoTime's diagnostic counters.

import { encodeForWatch, type PhoneToWatchMessage } from '@/protocol/gadgetbridge';
import { readDiagnostics } from '@/protocol/neotime';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const sendUsage = `neotime notify <title> <body> [--sender <name>]
neotime find [on|off]
neotime vibrate [count]
neotime send <json>
neotime line <text> [--wait <seconds>]
neotime diag

  notify, find and vibrate are the common Gadgetbridge messages; send takes
  any of them as JSON. These need a watch with the UART channel. line sends
  text as it is and prints what comes back. diag reads NeoTime's diagnostic
  counters.`;

export async function send(session: WatchSession, message: PhoneToWatchMessage): Promise<void> {
  await session.send(encodeForWatch(message));
}

// Send a line as it is and print whatever the watch says for a while.
export async function line(session: WatchSession, text: string, waitMs: number): Promise<void> {
  session.listen((reply) => say(reply));
  await session.send(`${text}\n`);
  await new Promise((resolve) => setTimeout(resolve, waitMs));
}

export async function diag(session: WatchSession): Promise<void> {
  for (const counter of await readDiagnostics(session)) {
    say(`${counter.label}: ${counter.value}`);
  }
}
