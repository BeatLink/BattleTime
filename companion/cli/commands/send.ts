// Send one Gadgetbridge message, and run a line on the watch REPL.

import { encodeForWatch, type PhoneToWatchMessage } from '@/protocol/gadgetbridge';

import type { WatchSession } from '../ble';
import { say } from '../output';

export const sendUsage = `waspos notify <title> <body> [--sender <name>]
waspos find [on|off]
waspos vibrate [milliseconds]
waspos send <json>
waspos repl <python> [--wait <seconds>]
waspos reset [--ota]

  notify, find and vibrate are the common Gadgetbridge messages; send takes
  any of them as JSON. repl runs a line on the watch and prints what comes
  back.`;

export async function send(session: WatchSession, message: PhoneToWatchMessage): Promise<void> {
  await session.send(encodeForWatch(message));
}

// Run a line on the watch REPL and print whatever it says for a while.
export async function repl(session: WatchSession, line: string, waitMs: number): Promise<void> {
  session.listen((reply) => say(reply));
  await session.send(`${line}\r\n`);
  await new Promise((resolve) => setTimeout(resolve, waitMs));
}

export async function reset(session: WatchSession, ota: boolean): Promise<void> {
  await session.send(`import machine\r\nmachine.${ota ? 'enter_ota_dfu' : 'reset'}()\r\n`);
  say(ota ? 'The watch is restarting in its bootloader.' : 'The watch is restarting.');
}
