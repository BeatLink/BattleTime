// Send one message over the UART channel, and read BattleTime's diagnostic counters.

import { appMessageFromData, encodeAppMessage } from '@/protocol/app-message';
import { encodeForWatch, type PhoneToWatchMessage } from '@/protocol/gadgetbridge';
import { crashSummary, decodeLastRestart } from '@/protocol/debug-report';
import { decodeDiagnostics, BATTLETIME_DIAGNOSTICS, BATTLETIME_SERVICE } from '@/protocol/battletime';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const sendUsage = `battletime notify <title> <body> [--sender <name>]
battletime find [on|off]
battletime vibrate [count]
battletime send <json>
battletime message <topic> <json>
battletime line <text> [--wait <seconds>]
battletime diag

  notify, find and vibrate are the common Gadgetbridge messages; send takes
  any of them as JSON. message sends an app message to the watch app whose
  id is the topic: up to 4 whole numbers and one text, as a JSON object.
  These need a watch with the UART channel. line sends
  text as it is and prints what comes back. diag reads BattleTime's diagnostic
  counters and why it last restarted.`;

export async function send(session: WatchSession, message: PhoneToWatchMessage): Promise<void> {
  await session.send(encodeForWatch(message));
}

// Send an app message to one watch app; throws before sending when the data breaks the limits.
export async function message(session: WatchSession, topic: string, json: string): Promise<void> {
  let data: unknown;
  try {
    data = JSON.parse(json);
  } catch {
    throw new Error('Give the message as a JSON object, such as \'{"on":0}\'.');
  }
  await session.send(encodeAppMessage(appMessageFromData(topic, data)));
}

// Send a line as it is and print whatever the watch says for a while.
export async function line(session: WatchSession, text: string, waitMs: number): Promise<void> {
  session.listen((reply) => say(reply));
  await session.send(`${text}\n`);
  await new Promise((resolve) => setTimeout(resolve, waitMs));
}

export async function diag(session: WatchSession): Promise<void> {
  const bytes = await session.read(BATTLETIME_SERVICE, BATTLETIME_DIAGNOSTICS);
  for (const counter of decodeDiagnostics(bytes)) {
    say(`${counter.label}: ${counter.value}`);
  }
  const lastRestart = decodeLastRestart(bytes);
  if (lastRestart) {
    say(`Last restart: ${lastRestart.resetReasons.join(', ')}`);
    say(crashSummary(lastRestart));
  }
}
