# NeoTime Companion

A React Native (Expo) phone, desktop and command line companion for watches running
[NeoTime](https://github.com/BeatLink/NeoTime), the Rust firmware for the PineTime. It started as
the wasp-os companion and keeps its screens, but talks to the watch through the services NeoTime
serves.

## Status

Early. What works today against NeoTime's firmware:

- scanning, connecting, and telling a running NeoTime from a watch waiting in a bootloader;
- firmware updates over Nordic legacy DFU, from a file or a GitHub release, then reconnecting so
  the watch keeps the new firmware instead of rolling it back;
- setting the clock and time zone over the standard Current Time service on every connection;
- reading NeoTime's diagnostic counters, and saving a debug report after a crash (the Debug report
  card on the Watch tab, or `neotime debug [file]`), which lets the watch leave its crash screen.

Built and tested against the mock watch, waiting for the firmware side
([companion-protocol.md](../docs/companion-protocol.md) in the watch's docs):

- the battery level from the standard Battery service;
- the Gadgetbridge-style messages over the Nordic UART Service: notifications, calls, music,
  weather, find, status, steps, alarms, watch settings and web requests for watch apps;
- the BLEFS file service: browsing files, backups, and installing compiled `.ntapp` apps.

Each feature is offered only when the connected watch serves what it needs, so nothing breaks while
the firmware catches up. On Android the app forwards notifications, calls and now playing, and obeys
Do Not Disturb; that bridge compiles but has not run on a phone yet, see
[src/services/phone/README.md](src/services/phone/README.md). The desktop app does the same with the
computer's own notifications, media players and Do Not Disturb, and can report the weather wherever
the computer is.

## Getting started

The app lives in `companion/` of the NeoTime repository. Everything it needs is in the
repository's flake; run these from `companion/`:

```bash
nix develop .#companion
npm install
```

One codebase builds three targets:

```bash
npm run android      # or npm run ios, a development build with real Bluetooth
npm run desktop      # the Electron app, with real Bluetooth through BlueZ
npm run tauri        # the Tauri desktop app, which is replacing Electron (see below)
npm start            # Expo, including the browser, where the mock watch stands in
```

Bluetooth on a phone needs a development build, so Expo Go falls back to the mock watch. A plain
browser has no Bluetooth either, and the mock is enough to work on screens without hardware.

The Android SDK, NDK and JDK live in a shell of their own, since they come to a few gigabytes:

```bash
nix develop .#companion-android
npm run android                       # build, install and start on a phone over adb
npx expo prebuild -p android && (cd android && ./gradlew assembleRelease)   # an APK, no phone needed
```

The shell pins the versions React Native 0.86 builds against (SDK 36, build tools 36.0.0, NDK
27.1.12297006, CMake 3.22.1) and points Gradle at the SDK's own `aapt2`, because the one Gradle
downloads does not run on NixOS.

### Installing

The flake also builds the desktop app and the command line tool as one package, with a menu entry:

```bash
nix run github:BeatLink/NeoTime#companion                # the desktop app
nix run github:BeatLink/NeoTime#companion-cli -- scan    # the command line tool
nix profile install github:BeatLink/NeoTime#companion    # neotime-companion and neotime on PATH
```

On NixOS it can go in `environment.systemPackages` or `home.packages` as
`inputs.neotime.packages.${system}.companion`. When `package-lock.json` changes,
`npmDepsHash` in the repository's `flake.nix` must change with it; `nix run nixpkgs#prefetch-npm-deps --
package-lock.json` prints the new one, and CI's Nix job fails until it matches.

```bash
npm test
npm run coverage     # the tests again, failing if coverage drops below the floors in package.json
npm run typecheck
npm run lint
```

CI runs the coverage, type check, lint and command line build on every push and pull request.

## The command line tool

Everything the app does to a watch can be done from a terminal on Linux, over the same BlueZ
transport the desktop app uses:

```bash
npm run neotime -- scan
npm run neotime -- flash path/to/firmware.zip
npm run neotime -- recovery path/to/recovery.zip
npm run neotime -- slots fit
npm run neotime -- time
npm run neotime -- diag
npm run neotime -- files ls /settings
npm run neotime -- note shopping.txt large
npm run neotime -- remote push
npm run neotime -- launcher style folders
npm run neotime -- apps setup
npm run neotime -- apps install path/to/timer.ntapp
npm run neotime -- apps list
npm run neotime -- apps hide dice
npm run neotime -- apps run path/to/prompt.ntapp --text text="Lunch now?"
npm run neotime -- ask "Lunch now?" --timeout 30
npm run neotime -- quick-apps set steps stopwatch heart
npm run neotime -- backup watch-backup.json
npm run neotime -- notify "Build finished" "All tests passed"
npm run neotime -- status
npm run neotime -- settings brightness=3
npm run neotime -- weather Kingston
```

The first scan remembers the watch in `~/.config/neotime-companion/cli.json`, so later commands
need no address; `--address` and `NEOTIME_ADDRESS` override it. `npm run neotime -- help` lists
every command. The tool runs the same code the screens run, with node-ble in place of the phone's
Bluetooth library, so each protocol exists only once.

## The desktop app

The desktop build is the same interface, rendered by React Native for Web and wrapped in Electron.
Only the transport differs. `electron/ble.js` runs in the main process and talks to BlueZ through
node-ble, and `electron/preload.js` exposes a narrow bridge to the window, so the interface never
touches Node. `src/ble/electron-transport.ts` implements the same `WatchTransport` interface the
phone uses, which is why every screen, every protocol and the tests are shared unchanged.

Electron comes from the flake rather than npm, because the binary npm downloads does not run on
NixOS. `npm run desktop` starts it through [scripts/electron.mjs](scripts/electron.mjs), which
skips the launcher npm would otherwise put first on `PATH`; that launcher starts Electron's
unwrapped binary, which dies with SIGILL. Run it inside `nix develop .#companion`, or the script says so.

### The Tauri desktop app

The desktop app is moving from Electron to [Tauri](https://tauri.app), so that the desktop, the
phone and the command line tool can share one Rust core with each other and, later, with the
firmware's own formats. `npm run tauri` exports the web build and starts it; `cargo test --workspace`
runs the Rust tests.

The window is the same web export. [tauri/src/bridge.js](tauri/src/bridge.js) gives it the same
`window.neotime` object the Electron preload did, so no screen changed. Behind it:

- [core/src/ble.rs](core/src/ble.rs) is the Bluetooth layer, on btleplug, with the same timeouts and
  link handling as `node/ble.js`. It has no Tauri in it, so the tool and the phone can reuse it.
  `cargo run -p neotime-companion-core --example probe` scans, connects and reads the battery and
  the time, to check it against a real watch.
- [tauri/src/desktop.rs](tauri/src/desktop.rs) reads notifications, MPRIS players, Do Not Disturb
  and GeoClue over D-Bus with zbus, sending the window the same raw events as `node/desktop.js`.
- [tauri/src/notes.rs](tauri/src/notes.rs) and [tauri/src/remote.rs](tauri/src/remote.rs) are the
  note file watch and the remote buttons' shell commands.

Set `NEOTIME_DEV_URL` to the Expo dev server's address to load the interface from it instead.

### The computer in place of a phone

On a phone the app forwards what the phone knows; in the desktop app the computer plays that part,
through [node/desktop.js](node/desktop.js) in the main process and
[src/services/phone/desktop.ts](src/services/phone/desktop.ts) in the window. Both builds share the
`phoneBridge` interface, so the watch provider and the screens do not know which one they have.

| On a phone | On Linux |
| --- | --- |
| Notifications | Desktop notifications, overheard on the session bus as apps send them |
| Now playing and media keys | Any MPRIS player: browsers, Spotify, VLC and the rest |
| Do Not Disturb | The notification server's `Inhibited` flag (KDE), or Cinnamon's, GNOME's or dunst's setting |
| Calls | None; the Calls toggle is hidden |
| The weather place | A place chosen by name, or wherever GeoClue says the computer is |
| The clock | The computer's clock |

The watch's clock is set on every connection and again whenever this device's clock jumps (set by
hand, synced, a new time zone, waking from sleep), and every six hours anyway, by
[src/state/clock-changes.ts](src/state/clock-changes.ts). That part runs on phones too.

Weather from the computer's location needs GeoClue running; on NixOS that is
`services.geoclue2.enable = true`, and GeoClue knows the app as `neotime-companion`.

### Connecting by address

The command line tool connects to a remembered address without scanning. BlueZ forgets an unpaired
watch soon after it last saw one, so when it does not know the address the tool runs discovery
until the watch appears, and says so if it never does. The desktop app uses the same Bluetooth
layer.

This and the rest of what is unfinished are listed in [TODO.md](TODO.md).

## Layout

```
src/app/                 Expo Router screens
  (tabs)/index.tsx       Watch: connection, status, diagnostics and quick actions
  (tabs)/firmware.tsx    Send a firmware package to the watch over DFU
  (tabs)/notifications   Forwarding toggles
  (tabs)/console.tsx     Raw UART traffic and a line to send as it is
  (tabs)/settings.tsx    App settings: weather place, hosts watch apps may fetch, GitHub builds
  scan.tsx               Modal that lists nearby watches
  apps.tsx               Install a compiled .ntapp image, hide or remove an app, see its memory
  setup.tsx              Set up a new watch: the shared icons, a face, the launcher and apps
  notes.tsx              The text file the watch's Notes app shows, and its font there
  remote.tsx             The watch's Remote app: its buttons, what each runs here, and recent taps
  launcher.tsx           The launcher's style, view and groups, and which group each app is in
  quick-apps.tsx         The apps a swipe left or right on the watch face steps through
  files.tsx              The watch's file system: browse, send, read, delete, free space
  backup.tsx             Copy the watch's settings and app state off and back
  steps.tsx              The last week of steps, by day and by hour
  alarms.tsx             The watch's alarms and the days they repeat on
  watch-settings.tsx     Brightness, face, timeout, units, step goal and theme on the watch
electron/                Desktop app: main process, preload bridge and file server
core/                    Rust core shared by every build: Bluetooth on btleplug, and the plugin host
tauri/                   Tauri desktop app: the window's bridge, D-Bus, note files, remote commands, plugins
plugins/                 Example plugins; npm run build-plugin -- plugins/<name> builds one
src/ble/                 Transport layer: react-native-ble-plx, Electron, and a mock for browsers
src/dfu/                 Firmware updates: Nordic legacy DFU and the package reader
cli/                     The command line tool, which runs the same code as the screens
node/                    BlueZ over node-ble, shared by the desktop app and the tool, and the
                         computer's notifications, media players and location over D-Bus
src/protocol/            NeoTime's GATT services, the file service, the Gadgetbridge messages,
                         backups and the requests the watch answers
src/services/            Weather, the watch apps' http proxy and GitHub firmware builds
src/services/phone/      Notifications, calls, media and Do Not Disturb from the phone or computer
modules/phone-bridge/    The Android native module behind it
src/state/               Watch provider (connection, capabilities, traffic, settings)
src/components/          Themed building blocks
```

## What the watch serves

The app reads which services the watch serves right after connecting, in
[src/protocol/neotime.ts](src/protocol/neotime.ts), and offers each feature only where it works:

| Service | UUID | Gives |
| --- | --- | --- |
| Legacy DFU | `00001530-1212-efde-1523-785feabcd123` | firmware updates |
| Current Time | `0x1805` / `0x2A2B`, `0x2A0F` | the clock and time zone, set on every connection |
| Battery | `0x180F` / `0x2A19` | the battery level |
| NeoTime diagnostics | `6e540001-…` / `6e540002-…` | eight 32-bit counters and the last restart with its crash; a written `01` says a debug report holds the crash |
| Device Information | `0x180A` / `0x2A26` | the firmware version, for debug reports |
| File transfer (BLEFS) | `0xFEBB` / `adaf0200-…` | files, backups, apps |
| NeoTime packages | `6e540001-…` / `6e540003-…` | installing, hiding and removing apps, the memory each needs and the watch's refusals |
| Nordic UART | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | the Gadgetbridge messages |

The NeoTime UUIDs share the base `6e54xxxx-0000-4000-8000-4e656f54696d`. The full description,
which the firmware and this app both follow, is [companion-protocol.md](../docs/companion-protocol.md) in the watch's docs.

| State | Tabs |
| --- | --- |
| No watch connected | Watch, Settings |
| NeoTime without the UART channel | Watch, Firmware, Settings |
| A watch with the UART channel | all of them |
| Waiting in a bootloader | Watch, Firmware, Settings |

A watch counts as running firmware when it serves the UART or NeoTime's own service, and as a
bootloader when it serves only DFU. NeoTime's advertisement has no room for its own 128-bit UUID,
so a scan recognises it by the name "NeoTime".

The mock watch comes as a NeoTime watch serving everything and a wasp-os watch waiting in its
bootloader, `DfuTarg (mock)`, so both states can be worked on with no hardware.

## Firmware updates

Firmware is sent as a Nordic DFU zip, the one NeoTime's `tools/package_dfu.py` makes from
`firmware.bin`. NeoTime takes the update in its running firmware, at the watch's own address, so
there is no reset into a bootloader first. After the image is activated the watch restarts, the
bootloader swaps it in, and the app reconnects and writes the time. That exchange is what makes
NeoTime keep the new firmware; without it the next reset rolls back to the old one.

A watch still on wasp-os is moved over by sending it NeoTime's `reloader-from-wasp-os.zip`. The
app restarts wasp-os into its bootloader with `machine.enter_ota_dfu()` over the UART, finds the
bootloader at the watch's address or the one above it, and sends the image there.

The controller in `src/dfu/legacy.ts` is a port of `tools/ota-dfu` from the wasp-os tree. The whole
flow runs against the stand-in watch in `src/dfu/mock-legacy-target.ts`, which is what the tests use
and what the Firmware tab talks to on web.

## Messages

Messages to the watch are the line `GB({...})`, as Gadgetbridge's Bangle.js driver sends them, so
the watch can serve this app and Gadgetbridge alike. The JSON is ASCII only, with anything else
escaped as `\uXXXX`. The watch replies with one JSON object per line, for example
`{"t":"music","n":"play"}`. The full list is in
[src/protocol/gadgetbridge.ts](src/protocol/gadgetbridge.ts).

Four messages are answered: `settings`, `alarm`, `steps` and `status` each come back with a reply of
the same name, which [src/protocol/requests.ts](src/protocol/requests.ts) matches to the request
waiting for it. A watch app asks for a page with `{"t":"http","url":..,"id":..}` and gets
`GB({"t":"http","id":..,"resp":..})` back, but only from the hosts listed in Settings.

## The file service

[src/protocol/file-service.ts](src/protocol/file-service.ts) speaks Adafruit's BLE file transfer
protocol, the file service InfiniTime calls `FSService`, on service `0xFEBB`. Each command is one
binary write to the transfer characteristic, and the watch answers with notifications on it: read
and read pacing, write and write data, delete, make folder, list folder and move. A write sends the
file in pieces no larger than the watch says it will take next, and a read asks again from where it
got to until it has the whole file. The watch makes missing folders when a file is written, and
deletes a folder with everything in it. The protocol has no free-space command, so `storage()`
starts a write to a scratch file, reads how much room the answer offers, and deletes the file.
Commands run one at a time, so answers never mix.

## Notes

The Notes screen keeps a text file of the user's choice on the watch, for NeoTime's `notes` app to
show read only. [src/protocol/notes.ts](src/protocol/notes.ts) writes it to `/notes/note.txt` and
the chosen font's name to `/notes/font` over the file service, each only when it differs from what
was last sent; the first check on a connection lists `/notes` too, so a note deleted on the watch
is sent again. [src/state/note-sync.ts](src/state/note-sync.ts) checks when the watch connects,
every minute, and when the file or font is changed here, one check at a time. Notes are limited to
64 kB.

[src/services/note-file.ts](src/services/note-file.ts) reaches the file. The desktop app picks it
in a file dialog and reads it from disk in the main process
([node/note-file.js](node/note-file.js)), which also watches its folder, so a save is sent within a
moment. A phone or a browser keeps the document picker's link and reads it again on each check; a
browser can read it only until the page reloads, and a phone may withdraw the app's access to it
after a restart, in which case the screen asks for the file again.

## Remote

The watch's `remote` app shows up to nine buttons and reports taps; the companion runs what each
button does. The Remote screen sets them up: each has an icon from the shared library, a label that
only the companion shows, and an action.

| Action | Where it runs | How |
| --- | --- | --- |
| Web request | everywhere | `fetch`, with the method, headers and body given, 15 s timeout |
| Command | desktop app | the user's shell in the Electron main process ([node/remote-command.js](node/remote-command.js)), 30 s timeout |
| Android | Android app | a link opened with `ACTION_VIEW`, an activity or a broadcast, through `modules/phone-bridge` |
| Ready-made | Android and desktop app | play/pause, play, pause, next, previous and volume through `phoneBridge.mediaCommand`; "find this device" everywhere, which shows the same card the watch's find phone does |

An action the device cannot run is marked unavailable in the screen and skipped, with the reason
in the screen's log, when tapped. [src/protocol/remote.ts](src/protocol/remote.ts) writes the
layout to `/remote/buttons` over the file service: byte 0 is the format version (1), byte 1 the
button count, then each button's icon as its place in the shared library's icon order. A tap
arrives as `{"t":"app","ch":"remote","d":{"button":<index>}}`; only that index is read, and the
action is always looked up in the companion's own settings
([src/services/remote-actions.ts](src/services/remote-actions.ts)), so the watch can never send a
command, URL or intent. [src/state/remote-sync.ts](src/state/remote-sync.ts) sends the layout when
a watch connects and on every change, as notes are kept.

The icon names are copied from `ICON_NAMES` in NeoTime's `compiler/src/types.rs` into
[src/bundles/icon-names.ts](src/bundles/icon-names.ts) with `npm run import-icon-names
path/to/NeoTime`; a test compares the two whenever a NeoTime checkout sits beside this one. The
picker draws each icon from the shared library image in the setup bundle
([src/protocol/library-icons.ts](src/protocol/library-icons.ts)), as the watch does.

`neotime remote list|push|run <n>|listen` does the same from a terminal, with the buttons in
`~/.config/neotime-companion/remote.json` (or `--buttons <file>`) in the form the app stores them.
In the browser's mock mode each button has a "Tap as the watch" button.

## Launcher

The watch's launcher groups the apps, and the Launcher screen arranges it. The **style** is how it
shows the groups: folders (a list of groups, each opening its apps), sections (one list with a
heading above each group), pages (a page per group, swiped left and right) or one plain list. The
**view** is how a group's apps are drawn: a row each, or a grid of icons. Groups can be renamed,
given a shared library icon, reordered, added and removed, and an app moved to another group or
to another place in its own.

Each app's image declares the group it starts in, such as `games`, and the `apps` reply carries it.
[src/protocol/launcher.ts](src/protocol/launcher.ts) works out where every installed app goes, each
in the group it declares until the wearer moves it, and writes that to `/settings/launcher` over the
file service as a few hundred bytes (the layout is in [companion-protocol.md](../docs/companion-protocol.md)), then
sends `{"c":"launcher"}` on the packages characteristic so the launcher shows it at once. Installing
an app writes the file again, so a new app lands in its group. The file lives on the watch, so
every phone and the command line see the same groups; with no file the screen starts from the
default groups: Games, Tools, Communication, Time, Phone, Health, System (settings included) and Other.
Setting up a watch writes those defaults, as folders. An app whose group is missing goes to Other,
and so does one installed another way, until the companion next writes the file.

`neotime launcher show|style <s>|view <v>|move <app> <group> [place]|add <name> [icon]|rename <group> <name>|icon <group> <icon>|order <group> <place>|remove <group>|reset`
does all of the same from a terminal.

## Quick apps

A swipe left or right on NeoTime's watch face opens the next or previous app in its quick-app
ring, and a step past either end comes back to the face. The watch reads the ring from
`/settings/quick_apps` on every step: UTF-8 text, one app id a line, in ring order, with blank
lines, surrounding spaces and `\r` ignored and ids that are not installed skipped. Without the
file the ring is `steps, stopwatch, heart, weather, music`.

The Quick apps screen reads the ring when it opens, falling back to that default when the file is
missing, and lists it next to the installed apps that could join it (faces and the launcher are
left out). Apps are added, removed and moved up or down, and Save writes the file over the file
service; "Use the default" deletes it. [src/protocol/quick-apps.ts](src/protocol/quick-apps.ts)
reads and writes the file, and `neotime quick-apps [list|set <id> ...|reset]` does the same from a
terminal.

## Apps and the first run

NeoTime ships with no apps, faces or icons: a new watch shows only a help screen. When a watch with
the packages characteristic has no face, the Watch tab offers **Set up the watch**, which installs
a face, the launcher and the apps chosen, in that order. The images come from
[src/bundles/watch-apps.ts](src/bundles/watch-apps.ts), generated from a folder of images compiled
with NeoTime's `neotime-compiler`: one `.ntapp` per app (the face as `face.ntapp`) and the library
from `neotime-compiler library -o shared.ntlib`. The images must match the firmware's runtime, so
rebuild the bundle when the widget or instruction numbering changes.

The shared library, with its icons, fonts and the accelerometer's configuration, ships inside the
firmware image, so a firmware update brings it and the companion never sends it. The bundle keeps
a copy only so the screens can draw the library's icons.

```bash
npm run import-watch-apps path/to/images
```

[src/protocol/packages.ts](src/protocol/packages.ts) installs one image: an app or face is written
to `/apps/<id>/app.ntapp` and then installed with `{"c":"install","id":...}` on the packages
characteristic, which checks it on the watch; a shared library image is refused, since it comes with
the firmware. The id and kind come from the image's
own header ([src/protocol/app-image.ts](src/protocol/app-image.ts)). The Apps screen and
`neotime apps` also list, hide, show and remove apps.

## Temporary apps

A temporary app is an app image the companion sends and opens at once, on top of whatever the watch
shows, without installing it. It never appears in the launcher, the Software app or the `apps`
listing, never saves state, and closes when the wearer goes back, when the companion sends `stop`
or when the link drops. `runTemporaryApp` in [src/protocol/packages.ts](src/protocol/packages.ts)
writes the image to `/temp/<id>/app.ntapp` (the id is the image's own) and each data file the app
reads to `/temp/<id>/<name>` (1 to 32 letters, digits, `-`, `_` and `.`, never `app.ntapp`, and at
most 4 kB each, since every run rewrites them). It deletes any other file in that folder, so the app
never reads data from an earlier run, then sends `{"c":"run","id":...}` on the packages
characteristic. The watch opens the app and only then replies, with the memory it needs, or with an
error when the image is missing or will not open, or with `why` 1 when the heap is too small. Faces
and libraries are refused here, before anything is sent. The files stay on the watch as a cache:
before sending the image the companion reads the first 20 bytes of the cached one, whose header holds
the CRC of the whole image, and sends it again only when those bytes or the length differ, so running
the same app again rewrites just its data. `{"c":"stop"}` closes the open temporary app. The watch
has no command to forget one, so `forgetTemporaryApp` sends `stop` and then deletes `/temp/<id>`
(or all of `/temp`) with the file transfer's delete.

The bundled `prompt` app ([src/protocol/prompt.ts](src/protocol/prompt.ts)) shows the text in
`/temp/prompt/text` with Yes and No buttons, sends `{"t":"app","ch":"prompt","d":{"answer":1}}`
(or `0`) for the tap and closes itself. Setting up a watch never installs it.

| Command | What it does |
| --- | --- |
| `neotime apps run <image> [--data <name>=<file>]... [--text <name>=<text>]...` | runs the image as a temporary app with those data files (4 kB at most each), saying whether the image was sent or already there |
| `neotime apps stop` | closes the open temporary app; its files stay |
| `neotime apps forget [id]` | closes the temporary app and deletes one temporary app's files, or all of them |
| `neotime ask "<question>" [--timeout <seconds>]` | asks with the prompt app and prints `yes` (exit 0) or `no` (exit 1); with no tap in time (60 s unless given) it closes the prompt and exits 2, and it exits 3 when it cannot ask, such as when the bundle has no `prompt.ntapp` |

## License

GPL v3 or later, like the rest of NeoTime. See [LICENSE](../LICENSE).
