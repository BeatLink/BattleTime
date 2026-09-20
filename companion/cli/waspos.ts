#!/usr/bin/env node
// The WaspOS Companion command line tool.
//
// Every command runs the same code the app does; only the Bluetooth layer
// differs, which is node-ble talking to BlueZ.

import { WatchSession, DEFAULT_SCAN_MS } from './ble';
import { apps, appsUsage } from './commands/apps';
import { flash, flashUsage } from './commands/flash';
import { scan, scanUsage } from './commands/scan';
import { repl, reset, send, sendUsage } from './commands/send';
import { say, warn } from './output';
import { resolveAddress } from './target';

type Options = {
  address?: string;
  sender?: string;
  timeout?: number;
  wait?: number;
  ota: boolean;
  verbose: boolean;
};

const USAGE = `waspos <command> [options]

${scanUsage}

${flashUsage}

${appsUsage}

${sendUsage}

Options:
  --address <mac>   Which watch to talk to. Otherwise the last one used, or
                    the only one in range. WASPOS_ADDRESS works too.
  --timeout <s>     How long to scan for.
  --verbose         Print every line to and from the watch.
`;

// Pull the named options out, leaving the positional arguments behind.
function parse(argv: string[]): { args: string[]; options: Options } {
  const args: string[] = [];
  const options: Options = { ota: false, verbose: false };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--address':
        options.address = argv[++i];
        break;
      case '--sender':
        options.sender = argv[++i];
        break;
      case '--timeout':
        options.timeout = Number(argv[++i]) * 1000;
        break;
      case '--wait':
        options.wait = Number(argv[++i]) * 1000;
        break;
      case '--ota':
        options.ota = true;
        break;
      case '--verbose':
      case '-v':
        options.verbose = true;
        break;
      default:
        args.push(arg);
    }
  }
  return { args, options };
}

// Whether a command needs a connected watch, as opposed to only the adapter.
const NEEDS_WATCH = new Set([
  'flash',
  'apps',
  'notify',
  'find',
  'vibrate',
  'send',
  'repl',
  'reset',
]);

async function main(argv: string[]): Promise<number> {
  const { args, options } = parse(argv);
  const [command, ...rest] = args;

  if (!command || command === 'help' || command === '--help' || command === '-h') {
    say(USAGE);
    return command ? 0 : 1;
  }

  const session = new WatchSession(options.verbose);
  try {
    if (command === 'scan') {
      await scan(session, options.timeout ?? DEFAULT_SCAN_MS);
      return 0;
    }

    if (!NEEDS_WATCH.has(command)) {
      warn(`Unknown command: ${command}`);
      say(USAGE);
      return 1;
    }

    const address = await resolveAddress(session, options.address);
    say(`Connecting to ${address}...`);
    await session.connect(address);

    switch (command) {
      case 'flash':
        if (!rest[0]) {
          throw new Error('Give the firmware package to send.');
        }
        await flash(session, rest[0]);
        break;

      case 'apps':
        await apps(session, rest);
        break;

      case 'notify':
        if (!rest[0]) {
          throw new Error('Give the notification title.');
        }
        await send(session, {
          t: 'notify',
          id: Date.now() % 100000,
          src: options.sender ?? 'WaspOS Companion',
          title: rest[0],
          body: rest[1] ?? '',
        });
        say('Sent.');
        break;

      case 'find':
        await send(session, { t: 'find', n: rest[0] !== 'off' });
        say(rest[0] === 'off' ? 'Stopped.' : 'The watch should be buzzing.');
        break;

      case 'vibrate':
        await send(session, { t: 'vibrate', n: Number(rest[0] ?? 100) });
        say('Sent.');
        break;

      case 'send':
        if (!rest[0]) {
          throw new Error('Give the message as JSON.');
        }
        await send(session, JSON.parse(rest[0]));
        say('Sent.');
        break;

      case 'repl':
        if (!rest[0]) {
          throw new Error('Give the line to run.');
        }
        await repl(session, rest[0], options.wait ?? 2000);
        break;

      case 'reset':
        await reset(session, options.ota);
        break;
    }
    return 0;
  } catch (error) {
    warn(`\n${(error as Error).message}`);
    return 1;
  } finally {
    await session.close();
  }
}

main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (error) => {
    warn(String(error));
    process.exit(1);
  },
);
