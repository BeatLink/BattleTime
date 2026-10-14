# The app store

The companion app's Store screen holds every app, watch face and plugin it can install, and installs
or uninstalls each one on the watch. It starts with every app and face in this repository, bundled
into the companion, and takes more from files and from GitHub. This page says how the store works,
what CI builds for it, and what a release carries.

## In the companion

Open it from the Watch tab (**Store**) or from the Apps screen (**Open the store**). Each entry
shows whether it is installed, its size and where it came from.

- **Install** sends an app or face to the watch, which checks it and refuses one that would not fit
  in memory, as installing from the Apps screen does. A plugin installs in the desktop app together
  with the watch apps it carries; on a phone, plugins are listed but cannot be installed, because
  they run only in the desktop app.
- **Uninstall** removes the app or face from the watch, with what it saved. For a plugin it removes
  the plugin's watch apps and the plugin.
- **Remove from the store** takes the entry out of the list. A bundled entry stays out until
  **Bring back the bundled entries** at the bottom of the screen.
- **Add from files** adds `.ntapp` and `.ntplugin` files, recognised by their contents.
- **Look on GitHub** lists the releases of a repository, `BeatLink/NeoTime` unless changed, that
  carry `neotime-store.zip` or loose `.ntapp` and `.ntplugin` files; with a GitHub token in Settings
  it lists the `neotime-store` artifacts of Actions runs too. Choosing one downloads it and offers
  each app and plugin in it, all switched on, to add.

An added entry replaces a bundled or earlier one with the same id, so adding a newer release
updates the store; installing it again then updates the watch. The store keeps each added file in
the app's storage, so installing needs no download. Apps and faces share the watch's ids and plugins
have their own, so an app and a plugin may share a name.

The setup screen for a new watch offers only part of the bundle (a face, the launcher and the apps
most people keep); the rest waits in the store.

The code is [companion/src/services/app-store.ts](../companion/src/services/app-store.ts) (entries,
keeping them, installing) and
[companion/src/services/github-store.ts](../companion/src/services/github-store.ts) (finding and
downloading), with the screen in [companion/src/app/store.tsx](../companion/src/app/store.tsx).

## From the command line

```bash
npm run neotime -- store list                          # the bundled apps and faces
npm run neotime -- store releases BeatLink/NeoTime     # releases with store files, numbered
npm run neotime -- store fetch BeatLink/NeoTime 1 apps # download the newest into apps/
npm run neotime -- apps install apps/dice.ntapp        # install a file
npm run neotime -- apps install dice                   # install the bundled dice app
```

`GITHUB_TOKEN` makes `releases` and `fetch` see Actions artifacts too. The command line keeps no
store of its own; plugins it fetches are installed in the desktop app.

## Building the store's files

[tools/build_store.py](../tools/build_store.py), inside `nix develop`, compiles every
`neotime/apps/<id>/app.tsx` except the template and every `neotime/faces/<id>/face.tsx`, packs every
plugin in `companion/plugins/` with `scripts/build-plugin.mjs` (which needs `npm ci` in
`companion/`, or pass `--no-plugins`), and writes `neotime/dist/store/`:

| File | What it is |
|---|---|
| `<id>.ntapp` | Each app and face, named by its id, which must match its folder. |
| `<id>.ntplugin` | Each plugin, with its watch app inside. |
| `store.json` | The list below. |
| `neotime-store.zip` | `store.json` and every app and plugin file, in one download. |
| `shared.ntlib` | The shared library, for the companion's bundle only; the firmware carries its own. |
| `SHA256SUMS` | The checksum of each file. |

`store.json` is format 1:

```json
{
  "format": 1,
  "version": "0.1.0",
  "commit": "71dc807",
  "apps": [
    { "id": "calculator", "name": "Calculator", "kind": "app", "group": "tools",
      "file": "calculator.ntapp", "bytes": 3376, "sha256": "…", "description": "A calculator, …" }
  ],
  "plugins": [
    { "id": "buzz-back", "name": "Buzz back", "version": "1.0.0", "description": "…",
      "watch": ["buzz-back"], "file": "buzz-back.ntplugin", "bytes": 1968, "sha256": "…" }
  ]
}
```

An app's description is the first paragraph of its page in [apps/](apps/), and a face's comes from
the table in [faces.md](faces.md). The companion refuses a `store.json` of another format. Another
repository can offer apps the same way: a release with `.ntapp` and `.ntplugin` files is enough, and
`neotime-store.zip` with `store.json` adds descriptions.

The companion's bundle is the same folder:

```bash
cd neotime && nix develop --command python3 -I ../tools/build_store.py
cd ../companion && npm run import-watch-apps ../neotime/dist/store
```

`import-watch-apps` marks the apps the setup screen offers; the list is at the top of
[companion/scripts/import-watch-apps.mjs](../companion/scripts/import-watch-apps.mjs).

## CI and releases

- [.github/workflows/store.yml](../.github/workflows/store.yml) builds the store whenever an app,
  face, plugin, the compiler, the runtime, the sdk or their docs change, and keeps it as the
  `neotime-store` artifact. On master it also moves the `apps-latest` pre-release to the commit and
  replaces its files, so the companion finds the newest apps without a token.
- [.github/workflows/release.yml](../.github/workflows/release.yml) runs for each `v*` tag. It builds
  the watch images with `tools/build_images.sh` and the store, and publishes both as the tag's
  release, with one `SHA256SUMS` over every file. The firmware's own files are described in
  [updates.md](updates.md).
