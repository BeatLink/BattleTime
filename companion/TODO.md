# Open items

What is known to be unfinished in this app. Remove an item in the same change that finishes it.

## Waiting for the firmware

The firmware side is described in `../docs/companion-protocol.md` and is being
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

## Temporary apps

- **No screen in the app.** Temporary apps run only from the command line (`neotime apps run`,
  `neotime ask`); the app has no screen for them yet.
- **Nothing sends one by itself.** No call, notification or plugin opens a temporary app yet, such
  as a prompt to answer a call or reply to a message.
- **The prompt image.** `prompt.ntapp` is not in the bundle until it is regenerated with
  `npm run import-watch-apps`; until then `neotime ask` exits 3.
- **Untried with a watch.** Running, the cached image check, stop and forget have run only against
  the mock watch. Check that a second run of the same image sends only its data, that old data
  files are deleted, that forgetting deletes the folder, and that a tap reaches `neotime ask` over
  BlueZ.

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
- **Steps and alarms from the watch's files.** The Steps and Alarms screens, `neotime steps` and
  `neotime alarms` have read `/steps/...` and read and written `/apps/alarm/state` only against the
  mock watch and the core's byte layout. Check on a watch that a changed alarm rings, that an open
  Alarm app redraws with it, and that the synced copy in the Tauri app refreshes after a write.
- **The NeoTime name check in scans.** A running NeoTime is told from a bootloader by its advertised
  name; check what a phone and BlueZ report for it.

## Notes

- **Untried with a watch.** Sending, resending after an edit, typed and URL notes and the font have
  run only against the mock watch, and the desktop app's file dialog and folder watch only in tests. With a watch on
  NeoTime and its `notes` app installed, check that:
  - a note typed on the screen is sent a moment after typing stops, and a URL's text is fetched
    and sent, and sent again within a minute of the page changing;
  - a URL behind a basic-auth login and a self-signed certificate is fetched with the certificate
    pinned, and refused with a different pin, in the Electron, Tauri and Android apps;
  - choosing a file in the desktop app sends it, and saving it in an editor sends it again within
    a few seconds;
  - on a phone, choosing a file sends it, and an edit is sent within a minute, also after the app
    restarts;
  - a new font reaches the watch, and the next opening of the app uses it;
  - a note over 64 kB is refused with its size, and "Stop and remove" brings back the app's hint;
  - `neotime note <file> large`, `neotime note text …`, `neotime note url …` and `neotime note remove`
    work over BlueZ.
- **Reading the file again on a phone.** The picker's `content://` link is read again with `fetch`,
  which may stop working after the app restarts; keeping access would need
  `takePersistableUriPermission` in the native module.

## QR code

- **Untried with a watch.** `/qr/list` and `/qr/<slot>` have run only against the mock watch.
  With the `qr-code` app installed, check that a sent code scans from the watch, also one near
  74 modules, and that adding, renaming, moving and removing codes and `neotime qr` work over BlueZ.

## Remote

- **Untried with a watch.** The layout file and taps have run only against the mock watch. Check
  that a pushed layout shows the right icons, in order, and that each tap runs its button.
- **Android intents.** `openUri`, `startActivity` and `sendBroadcast` in `PhoneBridgeModule.kt`
  compile, but have not run on a phone. Android 10 and later may refuse to start an activity
  while the app is in the background, which is where a tap usually finds it; broadcasts are not
  affected.
- **Web requests with a login or a pinned certificate.** Run against a local server in the Node and
  Rust tests only; check a button behind basic auth and a self-signed certificate in each app.
- **Media keys on a phone and the desktop.** The ready-made media actions reuse the forwarding
  bridge, which has not run against a real player from a tap yet.
- **The command line's buttons file.** `neotime remote` reads its own JSON file; the desktop app
  could export its buttons there.

## Quick apps

- **Untried with a watch.** Reading and writing `/settings/quick_apps` has run only against the
  mock watch, and the firmware's ring has not landed yet. Check that a saved ring changes what a
  swipe on the face opens at once, that "Use the default" brings back the built-in ring, and that
  `neotime quick-apps` works over BlueZ.

## Notification forwarding

The Android bridge in [src/services/phone](src/services/phone/README.md) is wired into the
Notifications tab and the watch provider, and compiles in a debug APK from `nix develop .#companion-android`,
but has never run on a phone. The keep-alive notification needs `POST_NOTIFICATIONS` asked for on
Android 13 and later.

## Tests

- **Screen and component tests.** `src/app` and `src/components` are left out of coverage because
  nothing tests them, which keeps the floor at 66%. Add `@testing-library/react-native`, test the
  screens against the mock transport, take them out of the `collectCoverageFrom` exclusions and
  raise the floors in `package.json`.
- **CLI commands.** `cli/commands/watch.ts`, `scan.ts` and `send.ts` are counted but barely tested;
  drive them against the mock watch as `cli/cli.test.ts` does for `flash`.

## Moving to Tauri

- **Connecting and updating from the Tauri app.** The Rust Bluetooth layer has scanned, connected
  and read GATT on the watch (the probe example), and the Tauri window scans through it; connecting,
  the UART lines and a firmware update from the window are untried.
- **Removing Electron.** `.#companion-tauri` packages the Tauri app; once connecting and a firmware
  update from its window have worked on a watch, it replaces `.#companion`, and Electron, `node/`
  and their npm dependencies go.
- **The command line tool in Rust**, on the core crate, once the protocols move there.
- **Protocols in the core crate**: the NeoTime service, packages, image header, launcher file and
  Gadgetbridge lines are in `core/src/protocol`, and BLEFS in `core/src/sync/files.rs`. Installing a
  plugin's watch apps runs on them; every other screen still runs the TypeScript ones. DFU is still to
  come, and the formats the firmware defines move into shared crates after.
- **Tauri on Android**, with the phone bridge as a Tauri plugin; try btleplug on a phone first.
- **Plugins on the phone.** [Plugins](../docs/plugins.md) run only in the desktop app, whose Rust
  core hosts them; the phone gets them with Tauri on Android.
- **Plugins on a real watch from the desktop app.** Buzz back has answered presses on a real watch
  through `cargo run -p neotime-companion-core --example plugin_host -- run <folder> <address>`, but
  not yet with the desktop app's own window connected.
- **Moving features into plugins.** Weather, notes, remote, alarms, steps, music and notification
  forwarding are still built into the app.

## Sync

[Sync](../docs/sync.md) mirrors the watch's settings, app state, steps and logs in the Rust core,
and lets plugins read them and change their own app's state.

- **Writes racing a closed app's run.** A closed app that comes due while the companion writes its
  state could save over the write. The shell runs it between Bluetooth requests, so the window is
  small but not closed.
- **Logs for heart rate, battery and sports sessions** under `/logs` on external flash, so there is
  history to sync. Sync already reads anything under `/logs`; each log needs a decoder in
  `core/src/sync/pass.rs`.
- **Untried with a watch.** The sync has run only against the core's fake watch.

## Not started

- **Firmware from GitHub Actions builds.** NeoTime has no CI yet; the finder already reads
  `firmware.zip` from releases and artifacts once there are some.
