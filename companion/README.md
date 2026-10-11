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
- the file service: browsing files, backups, and installing compiled `.ntapp` apps.

Each feature is offered only when the connected watch serves what it needs, so nothing breaks while
the firmware catches up. On Android the app forwards notifications, calls and now playing, and obeys
Do Not Disturb; that bridge has not been compiled yet, see
[src/services/phone/README.md](src/services/phone/README.md).

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

```bash
npm test
npm run typecheck
npm run lint
```

## The command line tool

Everything the app does to a watch can be done from a terminal on Linux, over the same BlueZ
transport the desktop app uses:

```bash
npm run neotime -- scan
npm run neotime -- flash path/to/firmware.zip
npm run neotime -- time
npm run neotime -- diag
npm run neotime -- files ls /settings
npm run neotime -- apps install path/to/timer.ntapp timer
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
  apps.tsx               Install a compiled .ntapp app, or remove one
  files.tsx              The watch's file system: browse, send, read, delete, free space
  backup.tsx             Copy the watch's settings and app state off and back
  steps.tsx              The last week of steps, by day and by hour
  alarms.tsx             The watch's alarms and the days they repeat on
  watch-settings.tsx     Brightness, face, timeout, units, step goal and theme on the watch
electron/                Desktop app: main process, preload bridge and file server
src/ble/                 Transport layer: react-native-ble-plx, Electron, and a mock for browsers
src/dfu/                 Firmware updates: Nordic legacy DFU and the package reader
cli/                     The command line tool, which runs the same code as the screens
node/                    BlueZ over node-ble, shared by the desktop app and the tool
src/protocol/            NeoTime's GATT services, the file service, the Gadgetbridge messages,
                         backups and the requests the watch answers
src/services/            Weather, the watch apps' http proxy and GitHub firmware builds
src/services/phone/      Notifications, calls, media and Do Not Disturb from the phone
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
| NeoTime file service | `6e540001-…` / `6e540003-…`, `6e540004-…` | files, backups, apps |
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

[src/protocol/file-service.ts](src/protocol/file-service.ts) sends JSON commands, one per line, on
the control characteristic: `ls`, `get`, `put`, `rm`, `df`, `install` and `remove`. File contents
travel as raw bytes on the data characteristic. During a `put` the watch reports how far it has got
every 4 KB, and the app waits for each report, so it never runs ahead of the flash. An app is
installed by writing its compiled image to `/apps/<id>/app.ntapp` and then asking the watch to check
and add it; removing it deletes the whole folder, saved state included.

## License

LGPL v3 or later. See [COPYING](COPYING) and [COPYING.LGPL](COPYING.LGPL).
