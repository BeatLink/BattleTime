# NeoTime Companion App

A React Native (Expo) phone app for watches running [NeoTime](../NeoTime), the wasp-os fork for
the PineTime. It talks to the watch over the Nordic UART Service using the same Gadgetbridge-style
JSON messages that NeoTime already understands.

## Status

Scaffolding only. The app can scan for a watch, connect, send the built-in test messages and show
raw traffic in a console. Forwarding real phone notifications and media state still needs native
code; see [src/services/notifications/README.md](src/services/notifications/README.md).

## Getting started

```bash
npm install
npx expo run:android   # or: npx expo run:ios
```

Bluetooth needs a development build, so Expo Go and the web target fall back to a mock watch.
That mock is enough to work on screens without hardware.

```bash
npx expo start --web   # UI only, mock transport
npm run typecheck
npm run lint
```

## Layout

```
src/app/                 Expo Router screens
  (tabs)/index.tsx       Watch: connection status and quick actions
  (tabs)/packages        Install, remove, enable and configure watch apps
  (tabs)/notifications   Forwarding toggles
  (tabs)/console.tsx     Raw traffic and a line into the watch REPL
  (tabs)/settings.tsx    App settings
  scan.tsx               Modal that lists nearby watches
src/app/configure.tsx    Settings form generated from a package's own schema
src/ble/                 Transport layer: react-native-ble-plx plus a mock for web and Expo Go
src/packages/            Bundled app packages and the catalogue that joins them to the watch
src/protocol/            Gadgetbridge and package manager message types, plus the transfer driver
src/state/               Watch provider (connection, traffic, settings) and persisted settings
src/components/          Themed building blocks
src/constants/theme.ts   Colours from the NeoTime design schema
```

## App packages

Watch apps are installed over Bluetooth without reflashing the firmware. Packages are built from
the NeoTime tree by its `tools/mkpkg.py`, then embedded in this app:

```bash
# in the NeoTime checkout
python3 tools/mkpkg.py apps/calculator apps/snake --out build-packages
# back here
npm run import-packages ../NeoTime/build-packages
```

The watch side is `wasp/pkgmgr.py` in the NeoTime tree, and the design is in its
`docs/app-packaging-design.md`. The mock watch answers package commands too, so the Apps tab can
be exercised on web with no hardware.

## Protocol

Messages to the watch are sent as the line of Python `GB({...})`, which the watch REPL evaluates
against `wasp/gadgetbridge.py` in the NeoTime tree. Gadgetbridge prefixes that line with `\x10`,
an Espruino convention that wasp-os does not use and that is unsafe to copy; see
`docs/app-packaging-references.md` in the NeoTime repo.

The watch replies with one JSON object per line, for example `{"t":"music","n":"play"}` or
`{"t":"findPhone","n":"true"}`. The full list of supported messages is in
[src/protocol/gadgetbridge.ts](src/protocol/gadgetbridge.ts).

## License

LGPL v3 or later, the same licence as wasp-os and NeoTime. See [COPYING](COPYING) and
[COPYING.LGPL](COPYING.LGPL).
