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
- setting the clock over the standard Current Time service on every connection;
- reading NeoTime's diagnostic counters.

Built and tested against the mock watch, waiting for the firmware side
([docs/companion-protocol.md](https://github.com/BeatLink/NeoTime/blob/master/docs/companion-protocol.md)
in the NeoTime tree):

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

Everything the project needs is in the flake:

```bash
nix develop
npm install
```

One codebase builds three targets:

```bash
npm run android      # or npm run ios, a development build with real Bluetooth
npm run desktop      # the Electron app, with real Bluetooth through BlueZ
npm start            # Expo, including the browser, where the mock watch stands in
```

Bluetooth on a phone needs a development build, so Expo Go falls back to the mock watch. A plain
browser has no Bluetooth either, and the mock is enough to work on screens without hardware.

The Android SDK, NDK and JDK live in a shell of their own, since they come to a few gigabytes:

```bash
nix develop .#android
npm run android                       # build, install and start on a phone over adb
npx expo prebuild -p android && (cd android && ./gradlew assembleRelease)   # an APK, no phone needed
```

The shell pins the versions React Native 0.86 builds against (SDK 36, build tools 36.0.0, NDK
27.1.12297006, CMake 3.22.1) and points Gradle at the SDK's own `aapt2`, because the one Gradle
downloads does not run on NixOS.

### Installing

The flake also builds the desktop app and the command line tool as one package, with a menu entry:

```bash
nix run github:BeatLink/neotime-companion                  # the desktop app
nix run github:BeatLink/neotime-companion#neotime -- scan  # the command line tool
nix profile install github:BeatLink/neotime-companion      # neotime-companion and neotime on PATH
```

On NixOS it can go in `environment.systemPackages` or `home.packages` as
`inputs.neotime-companion.packages.${system}.default`. When `package-lock.json` changes,
`npmDepsHash` in `flake.nix` must change with it; `nix run nixpkgs#prefetch-npm-deps --
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
npm run neotime -- time
npm run neotime -- diag
npm run neotime -- files ls /settings
npm run neotime -- note shopping.txt large
npm run neotime -- apps setup
npm run neotime -- apps install path/to/timer.ntapp
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
unwrapped binary, which dies with SIGILL. Run it inside `nix develop`, or the script says so.

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
  apps.tsx               Install a compiled .ntapp or .ntlib image, hide or remove an app
  setup.tsx              Set up a new watch: the shared icons, a face, the launcher and apps
  notes.tsx              The text file the watch's Notes app shows, and its font there
  files.tsx              The watch's file system: browse, send, read, delete, free space
  backup.tsx             Copy the watch's settings and app state off and back
  steps.tsx              The last week of steps, by day and by hour
  alarms.tsx             The watch's alarms and the days they repeat on
  watch-settings.tsx     Brightness, face, timeout, units, step goal and theme on the watch
electron/                Desktop app: main process, preload bridge and file server
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
| Current Time | `0x1805` / `0x2A2B` | the clock, set on every connection |
| Battery | `0x180F` / `0x2A19` | the battery level |
| NeoTime diagnostics | `6e540001-…` / `6e540002-…` | eight 32-bit counters |
| File transfer (BLEFS) | `0xFEBB` / `adaf0200-…` | files, backups, apps |
| NeoTime packages | `6e540001-…` / `6e540003-…` | installing, hiding and removing apps; the shared library |
| Nordic UART | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | the Gadgetbridge messages |

The NeoTime UUIDs share the base `6e54xxxx-0000-4000-8000-4e656f54696d`. The full description,
which the firmware and this app both follow, is `docs/companion-protocol.md` in the NeoTime tree.

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

## Apps and the first run

NeoTime ships with no apps, faces or icons: a new watch shows only a help screen. When a watch with
the packages characteristic has no face, the Watch tab offers **Set up the watch**, which installs
the shared library, a face, the launcher and the apps chosen, in that order. The images come from
[src/bundles/watch-apps.ts](src/bundles/watch-apps.ts), generated from a folder of images compiled
with NeoTime's `neotime-compiler`: one `.ntapp` per app (the face as `face.ntapp`) and the library
from `neotime-compiler library -o shared.ntlib`. The images must match the firmware's runtime, so
rebuild the bundle when the widget or instruction numbering changes.

```bash
npm run import-watch-apps path/to/images
```

[src/protocol/packages.ts](src/protocol/packages.ts) installs one image: an app or face is written
to `/apps/<id>/app.ntapp` and then installed with `{"c":"install","id":...}` on the packages
characteristic, which checks it on the watch; the shared library goes to `/resources/shared.ntlib`
and `{"c":"install_library"}` copies it into internal flash. The id and kind come from the image's
own header ([src/protocol/app-image.ts](src/protocol/app-image.ts)). The Apps screen and
`neotime apps` also list, hide, show and remove apps.

## License

LGPL v3 or later. See [COPYING](COPYING) and [COPYING.LGPL](COPYING.LGPL).
