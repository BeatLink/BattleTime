// Builds a plugin folder into a .ntplugin file the companion installs.
//
//   npm run build-plugin -- plugins/buzz-back [out-folder]
//
// The folder holds plugin.json (the manifest without file names), watch/app.tsx (optional, compiled
// with ../neotime's compiler), phone/main.ts (optional, bundled with esbuild) and phone/ui/ (optional,
// copied as it is, with index.html as the page). docs/plugins.md describes the result.
//
// A plugin for one of the watch's own apps names it with "app" instead of carrying watch/app.tsx, and
// takes plugins/shared's logic and page unless it has its own; "views" and "fields" in its plugin.json
// say what that page shows.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, relative, resolve } from 'node:path';

import { build } from 'esbuild';
import { zipSync } from 'fflate';

const [folderArgument, outArgument] = process.argv.slice(2);
if (!folderArgument) {
  console.error('usage: npm run build-plugin -- <plugin folder> [out folder]');
  process.exit(1);
}
const folder = resolve(folderArgument);
const out = resolve(outArgument ?? 'build/plugins');
const companion = resolve(import.meta.dirname, '..');
const neotime = resolve(companion, '..', 'neotime');

const { app, views = [], fields = {}, ...manifest } = JSON.parse(readFileSync(join(folder, 'plugin.json'), 'utf8'));
const files = {};
const shared = join(companion, 'plugins', 'shared');
if (app !== undefined && app !== manifest.id) {
  console.error(`${folder}: an app's plugin takes the app's id, ${app}.`);
  process.exit(1);
}

// The watch app, compiled to the image the watch runs.
const watchSource = app ? join(neotime, 'apps', app, 'app.tsx') : join(folder, 'watch', 'app.tsx');
manifest.watch = [];
if (existsSync(watchSource)) {
  const image = join(mkdtempSync(join(tmpdir(), 'neotime-plugin-')), `${manifest.id}.ntapp`);
  execFileSync(
    'cargo',
    ['run', '-q', '--release', '-p', 'neotime-compiler', '--', watchSource, '-o', image],
    { cwd: neotime, stdio: 'inherit' },
  );
  manifest.watch.push(`watch/${manifest.id}.ntapp`);
  files[`watch/${manifest.id}.ntapp`] = readFileSync(image);
}

// The logic, bundled into one script QuickJS runs as it is.
const ownMain = join(folder, 'phone', 'main.ts');
const mainSource = app && !existsSync(ownMain) ? join(shared, 'state-main.ts') : ownMain;
const filledIn = {
  __PLUGIN_ID__: JSON.stringify(manifest.id),
  __PLUGIN_NAME__: JSON.stringify(manifest.name),
  __PLUGIN_VIEWS__: JSON.stringify(views),
  __PLUGIN_FIELDS__: JSON.stringify(fields),
};
if (existsSync(mainSource)) {
  const bundled = await build({
    entryPoints: [mainSource],
    bundle: true,
    format: 'iife',
    target: 'es2020',
    write: false,
    logLevel: 'warning',
    define: filledIn,
  });
  manifest.main = 'phone/main.js';
  files['phone/main.js'] = bundled.outputFiles[0].contents;
}

// The settings page and every file beside it; the shared page's script is bundled beside it.
const ownUi = join(folder, 'phone', 'ui');
const uiFolder = app && !existsSync(join(ownUi, 'index.html')) ? join(shared, 'state-page') : ownUi;
if (existsSync(join(uiFolder, 'page.ts'))) {
  const bundled = await build({
    entryPoints: [join(uiFolder, 'page.ts')],
    bundle: true,
    format: 'iife',
    target: 'es2020',
    write: false,
    logLevel: 'warning',
    define: filledIn,
  });
  files['phone/ui/page.js'] = bundled.outputFiles[0].contents;
}
if (existsSync(join(uiFolder, 'index.html'))) {
  manifest.ui = 'phone/ui/index.html';
  const copy = (path) => {
    for (const name of readdirSync(path)) {
      const full = join(path, name);
      if (statSync(full).isDirectory()) {
        copy(full);
      } else if (!name.endsWith('.ts')) {
        files[`phone/ui/${relative(uiFolder, full).split('\\').join('/')}`] = readFileSync(full);
      }
    }
  };
  copy(uiFolder);
}

files['manifest.json'] = new TextEncoder().encode(`${JSON.stringify(manifest, null, 2)}\n`);
mkdirSync(out, { recursive: true });
const target = join(out, `${manifest.id}.ntplugin`);
writeFileSync(target, zipSync(files));
console.log(`Built ${relative(process.cwd(), target)} from ${basename(folder)}: ${Object.keys(files).sort().join(', ')}`);
