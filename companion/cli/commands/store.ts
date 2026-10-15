// The app store from the terminal: what the companion bundles, and the apps and plugins on GitHub releases.
// The terminal keeps no store of its own; fetched files land in a folder for `battletime apps install`.

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { bundledOffers, type StoreOffer } from '@/services/app-store';
import { fetchStoreRelease, listStoreReleases } from '@/services/github-store';

import { say } from '../output';

export const storeUsage = `battletime store [list]
battletime store releases [owner/name]
battletime store fetch [owner/name] [number] [folder]

  store list shows every app and face this tool bundles, which apps install takes by id. releases
  lists the GitHub releases carrying apps or plugins, newest first and numbered, with the Actions
  builds too when GITHUB_TOKEN is set. fetch downloads one of them, the newest unless numbered, into
  the folder, the current one unless given: each app as <id>.ntapp and each plugin as <id>.ntplugin.
  The repository is BeatLink/BattleTime unless given.`;

export const DEFAULT_STORE_REPO = 'BeatLink/BattleTime';

type StoreOptions = { token?: string; fetcher?: typeof fetch };

function describe(offer: StoreOffer): string {
  const { entry } = offer;
  const size = `${(entry.bytes / 1024).toFixed(1)} kB`;
  return `${entry.id.padEnd(16)} ${entry.kind.padEnd(6)} ${size.padStart(8)}  ${entry.name}`;
}

// A repository is owner/name; anything else is the next argument.
function splitRepo(args: string[]): [string, string[]] {
  return args[0]?.includes('/') ? [args[0], args.slice(1)] : [DEFAULT_STORE_REPO, args];
}

export function fileNameFor(offer: StoreOffer): string {
  return `${offer.entry.id}.${offer.entry.kind === 'plugin' ? 'ntplugin' : 'ntapp'}`;
}

export async function store(args: string[], options: StoreOptions = {}): Promise<void> {
  const [action = 'list', ...rest] = args;
  switch (action) {
    case 'list':
      for (const offer of bundledOffers()) say(describe(offer));
      return;
    case 'releases': {
      const [repo] = splitRepo(rest);
      const releases = await listStoreReleases(repo, options);
      if (releases.length === 0) say(`No release in ${repo} carries apps or plugins.`);
      releases.forEach((release, index) => {
        // A release's title already says when it is a pre-release; only a build needs naming.
        const where = release.source === 'actions' ? ' (Actions build)' : '';
        say(`${String(index + 1).padStart(2)}  ${release.date.slice(0, 10)}  ${release.title}${where}`);
      });
      return;
    }
    case 'fetch': {
      const [repo, more] = splitRepo(rest);
      const number = more[0] && /^\d+$/.test(more[0]) ? Number(more[0]) : 1;
      const folder = (more[0] && /^\d+$/.test(more[0]) ? more[1] : more[0]) ?? '.';
      const releases = await listStoreReleases(repo, options);
      const release = releases[number - 1];
      if (!release) throw new Error(`${repo} has no release numbered ${number} with apps or plugins.`);
      const offers = await fetchStoreRelease(repo, release, options);
      mkdirSync(folder, { recursive: true });
      for (const offer of offers) {
        writeFileSync(join(folder, fileNameFor(offer)), offer.data);
        say(describe(offer));
      }
      say(`Fetched ${offers.length} from ${release.title} into ${folder}.`);
      return;
    }
    default:
      throw new Error(`Unknown store action: ${action}\n${storeUsage}`);
  }
}
