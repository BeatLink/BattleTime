// The watch's remote app from the terminal: list the buttons, send their layout, run one, or answer taps.
//
// The buttons live in a JSON file holding the same list the app keeps in its settings.

import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

import { decodeWatchLine } from '@/protocol/gadgetbridge';
import { readRemoteButtons, REMOTE_BUTTONS_PATH, syncRemoteLayout, type RemoteButton } from '@/protocol/remote';
import {
  buttonName,
  describeAction,
  handleRemoteTap,
  runRemoteAction,
  unavailableReason,
  type CommandResult,
  type RemotePlatform,
} from '@/services/remote-actions';

import { runShellCommand } from '../../node/remote-command';
import type { WatchSession } from '../ble';
import { say } from '../output';


export const remoteUsage = `neotime remote [list|push|run <n>|listen] [--buttons <file>]

  remote reads the buttons from --buttons, or ~/.config/neotime-companion/remote.json: a JSON list of
  {"icon":"play","label":"Lights","action":{"kind":"http","method":"POST","url":"..."}} entries, as the
  app keeps them. list prints them, push writes their icons to ${REMOTE_BUTTONS_PATH} on the watch,
  run <n> runs button n here, and listen stays connected and runs each button tapped on the watch.
  Commands run in your shell; Android intents and media keys need the app.`;

// Subcommands that need no watch.
export const REMOTE_OFFLINE = new Set(['list', 'run']);

export function remoteButtonsPath(given?: string): string {
  if (given) return given;
  const base = process.env.XDG_CONFIG_HOME || join(homedir(), '.config');
  return join(base, 'neotime-companion', 'remote.json');
}

// The buttons in the file; a malformed entry is left out, as the app does.
export function loadRemoteButtons(path: string): RemoteButton[] {
  let raw: string;
  try {
    raw = readFileSync(path, 'utf8');
  } catch {
    throw new Error(`No remote buttons at ${path}. Write a JSON list of buttons there, or give --buttons.`);
  }
  return readRemoteButtons(JSON.parse(raw));
}

// What a terminal can do for a button: web requests and shell commands.
export function terminalPlatform(runCommand: (command: string) => Promise<CommandResult> = runShellCommand): RemotePlatform {
  return {
    fetch: (input, init) => fetch(input, init),
    runCommand,
    android: null,
    media: null,
    isPlaying: () => false,
    findPhone: () => say('The watch is looking for this computer.'),
  };
}

export async function remote(
  session: WatchSession | null,
  args: string[],
  buttonsFile: string | undefined,
  platform: RemotePlatform = terminalPlatform(),
) {
  const [action = 'list', first] = args;
  const buttons = loadRemoteButtons(remoteButtonsPath(buttonsFile));
  switch (action) {
    case 'list':
      if (buttons.length === 0) say('No buttons.');
      buttons.forEach((button, index) => {
        const reason = unavailableReason(button.action, platform);
        say(`${index + 1}. ${button.icon.padEnd(12)} ${buttonName(button)}  ${describeAction(button.action)}${reason ? '  (needs the app)' : ''}`);
      });
      return;
    case 'run': {
      const index = Number(first) - 1;
      const button = buttons[index];
      if (!Number.isInteger(index) || !button) {
        throw new Error(`Give a button number from 1 to ${buttons.length}.`);
      }
      const outcome = await runRemoteAction(button.action, platform);
      say(`${buttonName(button)}: ${outcome.message}`);
      if (!outcome.ok) throw new Error('The action failed.');
      return;
    }
    case 'push': {
      const result = await syncRemoteLayout(await session!.files(), buttons, session!.deviceId, null, false);
      say(`Sent ${buttons.length} buttons to ${REMOTE_BUTTONS_PATH}${result.sent ? '' : ' (unchanged)'}.`);
      return;
    }
    case 'listen':
      session!.listen((line) => {
        const message = decodeWatchLine(line);
        if (!message) return;
        void handleRemoteTap(message, buttons, platform).then((entry) => {
          if (entry) say(`${entry.label}: ${entry.message}`);
        });
      });
      say(`Answering taps on ${buttons.length} buttons. Press Ctrl+C to stop.`);
      await new Promise<void>((resolve) => process.once('SIGINT', () => resolve()));
      return;
    default:
      throw new Error(`Unknown remote action: ${action}`);
  }
}
