// Manage the app packages on the watch, the same commands the Apps tab
// sends.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

import { buildCatalog, decodeBundle } from '@/packages/catalog';
import type { PackageBundle, PackageMeta } from '@/protocol/packages';
import {
  installPackage,
  listPackages,
  readAbi,
  setEnabled,
  uninstallPackage,
  writeConfig,
} from '@/protocol/transfer';

import type { WatchSession } from '../ble';
import { ProgressBar, say } from '../output';

export const appsUsage = `waspos apps list
waspos apps install <name|directory>
waspos apps remove <name>
waspos apps enable <name>
waspos apps disable <name>
waspos apps config <name> <json>

  Installs from a package built by the firmware's tools/mkpkg.py, or by name
  from the packages bundled with this app.`;

// Read a package that mkpkg.py wrote out as a directory of files.
function readBundleFromDisk(directory: string): PackageBundle {
  const metaPath = join(directory, 'meta.json');
  const meta = JSON.parse(readFileSync(metaPath, 'utf8')) as PackageMeta;
  const files: PackageBundle['files'] = [];

  const walk = (at: string) => {
    for (const entry of readdirSync(at)) {
      const path = join(at, entry);
      if (statSync(path).isDirectory()) {
        walk(path);
        continue;
      }
      files.push({
        path: relative(directory, path).split(sep).join('/'),
        data: new Uint8Array(readFileSync(path)),
      });
    }
  };
  walk(directory);

  if (files.length === 0) {
    throw new Error(`${directory} holds no files to install.`);
  }
  return { name: meta.name, meta, files };
}

// A name matches one of the bundled packages; anything else is a path.
function bundleFor(target: string): PackageBundle {
  const entry = buildCatalog([], null).find((candidate) => candidate.bundle.name === target);
  if (entry) {
    return decodeBundle(entry.bundle);
  }
  return readBundleFromDisk(target);
}

export async function apps(session: WatchSession, args: string[]): Promise<void> {
  const channel = session.packageChannel;
  const [action, ...rest] = args;

  switch (action) {
    case undefined:
    case 'list': {
      const abi = await readAbi(channel);
      const installed = await listPackages(channel);
      say(`Firmware: bytecode ${abi.mpy ?? '?'}, architecture ${abi.arch ?? '?'}`);
      if (installed.length === 0) {
        say('No packages installed.');
        return;
      }
      for (const item of installed) {
        say(`${item.enabled ? '*' : ' '} ${item.name} ${item.version} (${item.kind})`);
      }
      return;
    }

    case 'install': {
      if (!rest[0]) {
        throw new Error('Name the package to install.');
      }
      const bundle = bundleFor(rest[0]);
      const abi = await readAbi(channel);
      const bar = new ProgressBar();
      await installPackage(channel, bundle, {
        abi,
        onProgress: (progress) =>
          bar.update(progress.sent, progress.total, progress.file.split('/').pop() ?? ''),
      });
      bar.done();
      say(`Installed ${bundle.name}.`);
      return;
    }

    case 'remove':
      if (!rest[0]) {
        throw new Error('Name the package to remove.');
      }
      await uninstallPackage(channel, rest[0]);
      say(`Removed ${rest[0]}.`);
      return;

    case 'enable':
    case 'disable': {
      if (!rest[0]) {
        throw new Error(`Name the package to ${action}.`);
      }
      await setEnabled(channel, rest[0], action === 'enable');
      say(`${action === 'enable' ? 'Enabled' : 'Disabled'} ${rest[0]}.`);
      return;
    }

    case 'config': {
      if (!rest[0] || !rest[1]) {
        throw new Error('Give a package name and its settings as JSON.');
      }
      await writeConfig(channel, rest[0], JSON.parse(rest[1]) as Record<string, unknown>);
      say(`Configured ${rest[0]}.`);
      return;
    }

    default:
      throw new Error(`Unknown apps command: ${action}`);
  }
}
