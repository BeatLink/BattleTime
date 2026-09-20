// Works out which watch a command is about, and remembers it so the next
// command does not have to scan again.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';

import { say, warn } from './output';
import { WatchSession } from './ble';

type Remembered = { address?: string; name?: string };

function configPath(): string {
  const base = process.env.XDG_CONFIG_HOME || join(homedir(), '.config');
  return join(base, 'waspos-companion', 'cli.json');
}

export function remembered(): Remembered {
  try {
    return JSON.parse(readFileSync(configPath(), 'utf8')) as Remembered;
  } catch {
    return {};
  }
}

export function remember(address: string, name: string) {
  const path = configPath();
  try {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, `${JSON.stringify({ address, name }, null, 2)}\n`);
  } catch (error) {
    warn(`Could not remember this watch: ${(error as Error).message}`);
  }
}

// An address given on the command line wins, then one in the environment,
// then the one last used, and only then does the tool go looking.
export async function resolveAddress(
  session: WatchSession,
  given: string | undefined,
): Promise<string> {
  const address = given ?? process.env.WASPOS_ADDRESS ?? remembered().address;
  if (address) {
    return address;
  }

  say('Looking for a watch...');
  const found = await session.scan();
  if (found.length === 0) {
    throw new Error('No watch found. Bring it closer, or give --address.');
  }
  if (found.length > 1) {
    for (const watch of found) {
      say(`  ${watch.id}  ${watch.name}`);
    }
    throw new Error('More than one watch is in range. Choose one with --address.');
  }
  remember(found[0].id, found[0].name);
  return found[0].id;
}
