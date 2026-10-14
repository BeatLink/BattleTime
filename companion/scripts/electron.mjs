// Starts the desktop app with the right Electron.
//
// npm puts node_modules/.bin first on PATH, so a plain `electron` would run
// the npm package's launcher. On NixOS that launcher starts the unwrapped
// binary, which dies with SIGILL. This finds the Electron the flake puts on
// PATH instead.

import { spawn } from 'node:child_process';
import { accessSync, constants } from 'node:fs';
import { delimiter, join } from 'node:path';

// Skip node_modules/.bin, which is the npm launcher rather than a real
// Electron.
function findElectron() {
  for (const directory of (process.env.PATH ?? '').split(delimiter)) {
    if (!directory || directory.includes('node_modules')) {
      continue;
    }
    const candidate = join(directory, 'electron');
    try {
      accessSync(candidate, constants.X_OK);
      return candidate;
    } catch {
      // Not here; keep looking.
    }
  }
  return null;
}

const electron = findElectron();
if (!electron) {
  process.stderr.write(
    'No Electron on PATH. Run `nix develop` first, which puts the one this app needs there.\n',
  );
  process.exit(1);
}

// The VSCode terminal exports this, and it makes Electron start as a bare
// Node process that exits at once.
const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;

const child = spawn(electron, ['electron/main.js', ...process.argv.slice(2)], {
  stdio: 'inherit',
  env,
});
child.on('exit', (code, signal) => process.exit(signal ? 1 : (code ?? 0)));
