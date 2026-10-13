# Open items

What is known to be unfinished in this app. Remove an item in the same change that finishes it.

## Waiting for the firmware

The firmware side is described in `docs/companion-protocol.md` in the NeoTime repo and is being
built in this order: the UART channel with the settings message, the Battery service, the remaining
Gadgetbridge messages. Each of these is built and tested here against the
mock watch only.

- **Theme and settings keys.** `src/protocol/theme.ts` and the watch settings screen still use the
  earlier eleven-colour layout. Switch them to NeoTime's keys once they are in the protocol doc.
- **Watch face list.** The settings reply names the faces; previews of NeoTime's faces could come
  from its `docs/images`.

## Packages

- **Choosing a face during setup.** The bundle holds the one face NeoTime builds; offer the others
  once `tools/build_watch_apps.sh` compiles them.
- **Updating a bundled app** that is already installed: compare versions and offer the newer one.

## Untried on a watch

- **Installing apps.** The firmware now serves the packages characteristic, but the Apps and setup
  screens have run only against the mock watch.
- **Files over BLEFS.** The client runs against the mock watch, and the firmware's service against a
  raw HCI client in the emulator, but the two have not met over a real radio.
- **Firmware updates against NeoTime.** The DFU flow and the reconnect that keeps the update run
  against the mock watch only; NeoTime's own Bluetooth controller has not run on hardware yet.
- **The phone and desktop builds.** They share the protocol code with the command line tool, but
  not its Bluetooth layer on a phone, and the desktop app has not been opened against a watch.
- **Forwarding from the desktop app.** Desktop notifications, MPRIS players and Do Not Disturb are
  read correctly on Cinnamon, but nothing has been forwarded to a real watch from the desktop yet.
  GeoClue's location is untried, since the development machine runs no GeoClue.
- **The NeoTime name check in scans.** A running NeoTime is told from a bootloader by its advertised
  name; check what a phone and BlueZ report for it.

## Notes

- **Untried with a watch.** Sending, resending after an edit and the font have run only against the
  mock watch, and the desktop app's file dialog and folder watch only in tests. With a watch on
  NeoTime and its `notes` app installed, check that:
  - choosing a file in the desktop app sends it, and saving it in an editor sends it again within
    a few seconds;
  - on a phone, choosing a file sends it, and an edit is sent within a minute, also after the app
    restarts;
  - a new font reaches the watch, and the next opening of the app uses it;
  - a note over 64 kB is refused with its size, and "Stop and remove" brings back the app's hint;
  - `neotime note <file> large` and `neotime note remove` work over BlueZ.
- **Reading the file again on a phone.** The picker's `content://` link is read again with `fetch`,
  which may stop working after the app restarts; keeping access would need
  `takePersistableUriPermission` in the native module.

## Remote

- **Untried with a watch.** The layout file and taps have run only against the mock watch. Check
  that a pushed layout shows the right icons, in order, and that each tap runs its button.
- **Android intents.** `openUri`, `startActivity` and `sendBroadcast` in `PhoneBridgeModule.kt`
  have not been compiled or run on a phone. Android 10 and later may refuse to start an activity
  while the app is in the background, which is where a tap usually finds it; broadcasts are not
  affected.
- **Media keys on a phone and the desktop.** The ready-made media actions reuse the forwarding
  bridge, which has not run against a real player from a tap yet.
- **The command line's buttons file.** `neotime remote` reads its own JSON file; the desktop app
  could export its buttons there.

## Notification forwarding

The Android bridge in [src/services/phone](src/services/phone/README.md) is wired into the
Notifications tab and the watch provider, and compiles in a debug APK from `nix develop .#android`,
but has never run on a phone. The keep-alive notification needs `POST_NOTIFICATIONS` asked for on
Android 13 and later.

## Tests

- **Screen and component tests.** `src/app` and `src/components` are left out of coverage because
  nothing tests them, which keeps the floor at 66%. Add `@testing-library/react-native`, test the
  screens against the mock transport, take them out of the `collectCoverageFrom` exclusions and
  raise the floors in `package.json`.
- **CLI commands.** `cli/commands/watch.ts`, `scan.ts` and `send.ts` are counted but barely tested;
  drive them against the mock watch as `cli/cli.test.ts` does for `flash`.

## Not started

- **Firmware from GitHub Actions builds.** NeoTime has no CI yet; the finder already reads
  `firmware.zip` from releases and artifacts once there are some.
