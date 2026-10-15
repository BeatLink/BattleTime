# Open items

What is known to be unfinished in this app. Remove an item in the same change that finishes it.

## Waiting for the firmware

The firmware side is described in `../docs/companion-protocol.md` and is being
built in this order: the UART channel with the settings message, the Battery service, the remaining
Gadgetbridge messages. Each of these is built and tested here against the
mock watch only.

- **Theme and settings keys.** `src/protocol/theme.ts` and the watch settings screen still use the
  earlier eleven-colour layout. Switch them to BattleTime's keys once they are in the protocol doc.
- **Watch face list.** The settings reply names the faces; previews of BattleTime's faces could come
  from its `docs/images`.

## Packages

- **Choosing a face during setup.** The bundle holds every face, but setup marks only `digital` for
  a new watch; let the user pick which face to start with.
- **Updating a bundled app** that is already installed: compare versions and offer the newer one.

## Temporary apps

- **No screen in the app.** Temporary apps run only from the command line (`battletime apps run`,
  `battletime ask`); the app has no screen for them yet.
- **Nothing sends one by itself.** No call, notification or plugin opens a temporary app yet, such
  as a prompt to answer a call or reply to a message.
- **Untried with a watch.** Running, the cached image check, stop and forget have run only against
  the mock watch. Check that a second run of the same image sends only its data, that old data
  files are deleted, that forgetting deletes the folder, and that a tap reaches `battletime ask` over
  BlueZ.

## Untried on a watch

- **Installing apps.** The firmware now serves the packages characteristic, but the Apps and setup
  screens have run only against the mock watch.
- **Files over BLEFS.** The client runs against the mock watch, and the firmware's service against a
  raw HCI client in the emulator, but the two have not met over a real radio.
- **Firmware updates against BattleTime.** The DFU flow and the reconnect that keeps the update run
  against the mock watch only; BattleTime's own Bluetooth controller has not run on hardware yet.
- **The app's window on a watch.** It shares the protocol code with the command line tool, but its
  Bluetooth runs in the Rust core; see "The app" below for what it has done on the real watch.
- **Forwarding from the desktop app.** Desktop notifications, MPRIS players and Do Not Disturb are
  read correctly on Cinnamon, but nothing has been forwarded to a real watch from the desktop yet.
  GeoClue's location is untried, since the development machine runs no GeoClue.
- **Steps and alarms from the watch's files.** The Steps and Alarms screens, `battletime steps` and
  `battletime alarms` have read `/day_blocks/steps/...` and `/steps/...`, and read and written `/apps/alarm/state` only against the
  mock watch and the core's byte layout. Check on a watch that a changed alarm rings, that an open
  Alarm app redraws with it, and that the synced copy in the Tauri app refreshes after a write.
- **The BattleTime name check in scans.** A running BattleTime is told from a bootloader by its advertised
  name; check what a phone and BlueZ report for it.

## Notes

- **Untried with a watch.** Sending, resending after an edit, typed and URL notes and the font have
  run only against the mock watch, and the desktop app's file dialog and folder watch only in tests. With a watch on
  BattleTime and its `notes` app installed, check that:
  - a note typed on the screen is sent a moment after typing stops, and a URL's text is fetched
    and sent, and sent again within a minute of the page changing;
  - a URL behind a basic-auth login and a self-signed certificate is fetched with the certificate
    pinned, and refused with a different pin, on the desktop and on Android;
  - choosing a file in the desktop app sends it, and saving it in an editor sends it again within
    a few seconds;
  - on a phone, choosing a file sends it, and an edit is sent within a minute, also after the app
    restarts;
  - a new font reaches the watch, and the next opening of the app uses it;
  - a note over 64 kB is refused with its size, and "Stop and remove" brings back the app's hint;
  - `battletime note <file> large`, `battletime note text …`, `battletime note url …` and `battletime note remove`
    work over BlueZ.
- **Reading the file again on a phone.** The picker's `content://` link is read again by the core,
  which may stop working after the app restarts; keeping access would need
  `takePersistableUriPermission` in the phone bridge plugin.

## QR code

- **Untried with a watch.** `/qr/list` and `/qr/<slot>` have run only against the mock watch.
  With the `qr-code` app installed, check that a sent code scans from the watch, also one near
  74 modules, and that adding, renaming, moving and removing codes and `battletime qr` work over BlueZ.

## Remote

- **Untried with a watch.** The layout file and taps have run only against the mock watch. Check
  that a pushed layout shows the right icons, in order, and that each tap runs its button.
- **Android intents.** `openUri`, `startActivity` and `sendBroadcast` in `PhoneBridgePlugin.kt`
  compile, but have not run on a phone. Android 10 and later may refuse to start an activity
  while the app is in the background, which is where a tap usually finds it; broadcasts are not
  affected.
- **Web requests with a login or a pinned certificate.** Run against a local server in the Node and
  Rust tests only; check a button behind basic auth and a self-signed certificate in each app.
- **Media keys on a phone and the desktop.** The ready-made media actions reuse the forwarding
  bridge, which has not run against a real player from a tap yet.
- **The command line's buttons file.** `battletime remote` reads its own JSON file; the desktop app
  could export its buttons there.

## Quick apps

- **Untried with a watch.** Reading and writing `/settings/quick_apps` has run only against the
  mock watch, and the firmware's ring has not landed yet. Check that a saved ring changes what a
  swipe on the face opens at once, that "Use the default" brings back the built-in ring, and that
  `battletime quick-apps` works over BlueZ.

## Notification forwarding

The Android bridge in [src/services/phone](src/services/phone/README.md) is wired into the
Notifications tab and the watch provider, and compiles in a debug APK from `nix develop .#companion-android`,
but has never run on a phone. The keep-alive notification needs `POST_NOTIFICATIONS` asked for on
Android 13 and later.

## Tests

- **Screen and component tests.** `src/screens` and `src/components` are left out of coverage
  because nothing tests them. Render the screens with Preact in jsdom against the mock transport,
  take them out of the `collectCoverageFrom` exclusions and raise the floors in `package.json`.
- **CLI commands.** `cli/commands/watch.ts`, `scan.ts` and `send.ts` are counted but barely tested;
  drive them against the mock watch as `cli/cli.test.ts` does for `flash`.

## The app

- **Message lines and a firmware update from the window.** The window has scanned, connected
  and installed a plugin's watch app on the real watch; it has not yet sent or received the
  Gadgetbridge lines, nor run a firmware update.
- **The Preact window on a watch.** The window was rewritten from React Native to Preact and Vite
  and checked only against the mock watch, in a browser; open every screen against the real watch.
- **Saving files on Android.** Backups, debug reports and file downloads use the web view's own
  download, which Android's WebView may ignore; save them through the core or a share sheet there.
- **Buzz back's answer felt on the wrist.** In the `plugin_host` test the plugin answered four presses
  with a two-buzz message and the write went out without error, but whether the watch buzzed twice
  each time was not confirmed.
- **The watch drops an idle link.** Both the window and the `plugin_host` example lost the
  connection within about half a minute of connecting with nothing to send, and neither reconnects.
- **The command line tool in Rust**, on the core crate, once the protocols move there.
- **Protocols in the core crate**: the BattleTime service, packages, image header, launcher file and
  Gadgetbridge lines are in `core/src/protocol`, and BLEFS in `core/src/sync/files.rs`. Installing a
  plugin's watch apps runs on them; every other screen still runs the TypeScript ones. DFU is still to
  come, and the formats the firmware defines move into shared crates after.
- **The app on Android is untried on a phone.** Its debug APK builds, with btleplug, the
  phone bridge plugin, sync and plugins in it, but it has never been installed. On a phone, check
  that Bluetooth starts (logcat says "Bluetooth could not start" if not), that the nearby devices
  prompt appears and a scan, a connection and a firmware update work; that notifications, calls,
  media, Do Not Disturb, the location and the remote buttons' intents reach the window; that sync
  and an installed plugin run; and that a note picked as a `content://` link is still readable
  after a restart.
- **Moving features into plugins.** Weather, notes, remote, alarms, steps, music and notification
  forwarding are still built into the app.

## Sync

[Sync](../docs/sync.md) mirrors the watch's settings, app state, steps and logs in the Rust core,
and lets plugins read them and change their own app's state.

- **Untried with a watch.** The sync has run only against the core's fake watch.

## Linked companions

[Linked companions](../docs/linked-companions.md) lets the phone hand the watch to the desktop at
home and keeps the two companions' settings in step. Nothing is built yet.

- **Finding and pairing:** mDNS, the QR code and the TLS link, with a screen for linked devices.
- **Sync areas:** the area table, record versions and the swap on linking, starting with remote
  buttons and the watch's copy.
- **Handoff:** the phone lets go and the desktop takes the watch, with the 15-second fallback.
- **Relaying:** the phone bridge's second source, and taps and replies sent back to the phone.
- **The other areas:** plugins, notes and the rest.
