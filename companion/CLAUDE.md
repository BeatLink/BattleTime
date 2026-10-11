# NeoTime Companion

Expo (React Native) companion app for watches running the NeoTime firmware.
Read [README.md](README.md) for the layout and the protocol.

- Screens live in `src/app` (Expo Router). Bluetooth stays behind `src/ble/transport.ts`; screens
  use the `useWatch` hook and never import a Bluetooth library directly. There are three
  transports: react-native-ble-plx on a phone, Electron over BlueZ on the desktop, and a mock in
  browsers and Expo Go.
- `electron/` and `cli/` run in Node, not in the bundle, and Metro never sees them. Both talk to
  BlueZ through `node/ble.js`, so a change to Bluetooth on a computer belongs there. Every step
  there has a timeout and a failed connect drops the link, because a half-open BlueZ connection
  makes every later attempt wait for services that never resolve.
- `cli/` is the command line tool. It imports the same `src/` modules the screens do, so a feature
  gets a command rather than a second implementation. `npm run neotime -- <command>` rebuilds it
  with esbuild and runs it.
- Raw GATT goes through the `DfuLink` each transport supplies with `dfuLink()`. The DFU controller,
  `src/protocol/neotime.ts` (time, battery, diagnostics, capabilities) and
  `src/protocol/file-service.ts` all use it and never touch a Bluetooth library.
  `src/dfu/mock-legacy-target.ts` is the mock watch behind it, NeoTime or wasp-os.
- A feature is offered only when the watch serves what it needs. `useWatch` gives `watchReady`
  (connected and running firmware) and `capabilities` (`dfu`, `time`, `battery`, `diagnostics`,
  `files`, `uart`); use those, not `connection`, to decide whether a feature is shown.
- The phone-to-watch protocol is fixed in `docs/companion-protocol.md` in the NeoTime repo, agreed
  with the firmware side. `src/protocol/neotime.ts`, `file-service.ts`, `gadgetbridge.ts` and
  `requests.ts` mirror it; change it there first, then here.
- After a firmware update the app must exchange GATT data with the watch (it writes the time), or
  NeoTime rolls the update back at its next reset.
- Use `nix develop` for the toolchain. Electron must come from the flake, not from npm.
- Open work is in [TODO.md](TODO.md); remove an item there in the same change that finishes it.
- Verify with `npm test`, `npm run typecheck` and `npm run lint`. BLE needs a dev build; web and
  Expo Go use the mock transport.
