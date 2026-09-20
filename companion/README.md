# WaspOS Companion

A React Native (Expo) phone app for watches running [wasp-os](https://github.com/wasp-os/wasp-os),
the MicroPython firmware for the PineTime. It talks to the watch over the Nordic UART Service using
the same Gadgetbridge-style JSON messages the firmware already understands.

## Status

Scaffolding only. The app can scan for a watch, connect, install app packages, flash firmware, send
the built-in test messages and show raw traffic in a console. Forwarding real phone notifications
and media state still needs native code; see
[src/services/notifications/README.md](src/services/notifications/README.md).

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
npm run waspos -- scan
npm run waspos -- flash path/to/micropython.zip
npm run waspos -- apps list
npm run waspos -- apps install path/to/wasp-os/build-packages/calculator
npm run waspos -- notify "Build finished" "All tests passed"
npm run waspos -- repl "wasp.system.brightness"
npm run waspos -- diag
npm run waspos -- reset --ota
```

The first scan remembers the watch in `~/.config/waspos-companion/cli.json`, so later commands
need no address; `--address` and `WASPOS_ADDRESS` override it. `npm run waspos -- help` lists
every command. The tool runs the same code the screens run, with node-ble in place of the phone's
Bluetooth library, so each protocol exists only once.

## The desktop app

The desktop build is the same interface, rendered by React Native for Web and wrapped in Electron.
Only the transport differs. `electron/ble.js` runs in the main process and talks to BlueZ through
node-ble, and `electron/preload.js` exposes a narrow bridge to the window, so the interface never
touches Node. `src/ble/electron-transport.ts` implements the same `WatchTransport` interface the
phone uses, which is why every screen, the package protocol and the tests are shared unchanged.

Electron comes from the flake rather than npm, because the binary npm downloads does not run on
NixOS. `npm run desktop` starts it through [scripts/electron.mjs](scripts/electron.mjs), which
skips the launcher npm would otherwise put first on `PATH`; that launcher starts Electron's
unwrapped binary, which dies with SIGILL. Run it inside `nix develop`, or the script says so.

## Layout

```
src/app/                 Expo Router screens
  (tabs)/index.tsx       Watch: connection status and quick actions
  (tabs)/packages        Install, remove, enable and configure watch apps
  (tabs)/firmware.tsx    Send a firmware package to the watch over DFU
  (tabs)/notifications   Forwarding toggles
  (tabs)/console.tsx     Raw traffic, quick commands and a line into the watch REPL
  (tabs)/settings.tsx    App settings
  scan.tsx               Modal that lists nearby watches
src/app/configure.tsx    Settings form generated from a package's own schema
electron/                Desktop app: main process, preload bridge and file server
src/ble/                 Transport layer: react-native-ble-plx, Electron, and a mock for browsers
src/dfu/                 Firmware updates: the Nordic DFU protocols and the package reader
cli/                     The command line tool, which runs the same code as the screens
node/                    BlueZ over node-ble, shared by the desktop app and the tool
src/packages/            Bundled app packages and the catalogue that joins them to the watch
src/protocol/            Gadgetbridge and package manager message types, plus the transfer driver
src/state/               Watch provider (connection, traffic, settings) and persisted settings
src/components/          Themed building blocks
src/constants/theme.ts   Colours from the wasp-os design schema
```

## App packages

Watch apps are installed over Bluetooth without reflashing the firmware. Packages are built from
the wasp-os tree by its `tools/mkpkg.py`, then embedded in this app:

```bash
# in the wasp-os checkout
python3 tools/mkpkg.py apps/calculator apps/snake --out build-packages
# back here
npm run import-packages path/to/wasp-os/build-packages
```

The watch side is `wasp/pkgmgr.py` in the wasp-os tree, and the design is in its
`docs/app-packaging-design.md`. The mock watch answers package commands too, so the Apps tab can
be exercised on web with no hardware.

## What each state shows

A screen that needs a watch is left out of the tab bar rather than shown as something that does
not work, so the app only ever offers what it can actually do:

| State | Tabs |
| --- | --- |
| No watch connected | Watch, Settings |
| Connected, firmware running | all of them |
| Connected, waiting in a bootloader | Watch, Firmware, Settings |

A watch in its bootloader runs no firmware, so it answers no Gadgetbridge message and has no
package manager and no REPL. Both transports notice it the same way: the scan looks for a DFU
service as well as the UART one and marks what it finds, and a connection that asks for the UART
service and does not get one reports `bootloader` rather than failing. That is what makes an
interrupted update recoverable, because the watch is left exactly there.

The mock watch has a bootloader twin, `DfuTarg (mock)`, so both states can be worked on with no
hardware.

## Firmware updates

Firmware is sent as the Nordic DFU zip the wasp-os build produces, which holds an init packet and
an image for each part of the firmware. The Firmware tab reads the package, resets the watch into
its bootloader with `machine.enter_ota_dfu()`, reconnects, and sends the images.

Both Nordic protocols are implemented, because which one answers depends on the bootloader the
watch was flashed with:

- **Legacy** (`src/dfu/legacy.ts`) is what the PineTime's own bootloader speaks. The controller
  reconnects at the watch's address.
- **Secure** (`src/dfu/secure.ts`) is the nRF5 SDK 12 and later protocol. Its bootloader advertises
  one address above the watch, and every object is checked against a CRC-32 before it is executed.

`src/dfu/flash.ts` picks between them by looking for each control point, and only falls back to the
buttonless characteristic when the watch cannot be reset over the UART line. The whole flow runs
against the stand-in bootloaders in `src/dfu/mock-target.ts` and `src/dfu/mock-legacy-target.ts`,
which is what the tests use and what the Firmware tab talks to on web.

The reference implementation is `tools/ota-dfu` in the wasp-os tree; these controllers are a port
of it.

## Protocol

Messages to the watch are sent as the line of Python `GB({...})`, which the watch REPL evaluates
against `wasp/gadgetbridge.py` in the wasp-os tree. Gadgetbridge prefixes that line with `\x10`,
an Espruino convention that wasp-os does not use and that is unsafe to copy; see
`docs/app-packaging-references.md` in the wasp-os repo.

The watch replies with one JSON object per line, for example `{"t":"music","n":"play"}` or
`{"t":"findPhone","n":"true"}`. The full list of supported messages is in
[src/protocol/gadgetbridge.ts](src/protocol/gadgetbridge.ts).

## License

LGPL v3 or later, the same licence as wasp-os. See [COPYING](COPYING) and
[COPYING.LGPL](COPYING.LGPL).
