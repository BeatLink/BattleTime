# BattleTime Companion

The companion app for watches running [BattleTime](https://github.com/BeatLink/BattleTime), the Rust
firmware for the PineTime: one [Tauri](https://tauri.app) app for the desktop and Android, and a
command line tool. It talks to the watch through the services BattleTime serves, and supports no other
firmware beyond sending a watch the reloader that moves it onto BattleTime.

How to install and use it is in the user guide, [docs/companion](../docs/companion/index.md):
[connecting a watch](../docs/companion/connecting.md), [apps and the store](../docs/companion/apps-and-store.md),
[phone features](../docs/companion/phone-features.md), [the watch's data](../docs/companion/watch-data.md),
[firmware updates](../docs/companion/firmware-updates.md), [notes, QR codes, survival guides, remote and tools](../docs/companion/tools.md)
and [the command line](../docs/companion/command-line.md). This file is for working on the app.

## Status

Early. What works today against BattleTime's firmware:

- scanning, connecting, and telling a running BattleTime from a watch waiting in a bootloader;
- firmware updates over Nordic legacy DFU, from a file or a GitHub release, then reconnecting so
  the watch keeps the new firmware instead of rolling it back;
- setting the clock and time zone over the standard Current Time service on every connection;
- reading BattleTime's diagnostic counters, and saving a debug report after a crash (the Debug report
  card on the Watch tab, or `battletime debug [file]`), which lets the watch leave its crash screen.

Built and tested against the mock watch, waiting for the firmware side
([companion-protocol.md](../docs/companion-protocol.md) in the watch's docs):

- the battery level from the standard Battery service;
- the Gadgetbridge-style messages over the Nordic UART Service: notifications, calls, music,
  weather, find, status, watch settings and web requests for watch apps;
- the BLEFS file service: browsing files, backups, installing compiled `.ntapp` apps, and the step
  logs and the alarm app's state behind the Steps and Alarms screens.

Each feature is offered only when the connected watch serves what it needs, so nothing breaks while
the firmware catches up. On Android the app forwards notifications, calls and now playing, and obeys
Do Not Disturb; that bridge compiles but has not run on a phone yet, see
[src/services/phone/README.md](src/services/phone/README.md). On the desktop the computer does the
same with its own notifications, media players and Do Not Disturb, and can report the weather
wherever the computer is. What is unfinished is listed in [TODO.md](TODO.md).

## Getting started

Everything the app needs is in the repository's flake; run these from `companion/`:

```bash
nix develop .#companion
npm ci
npm run tauri        # build the window with Vite and open the desktop app
npm run dev          # the window alone in a browser, where the mock watch stands in for Bluetooth
```

```bash
npm test
npm run coverage     # the tests again, failing if coverage drops below the floors in package.json
npm run typecheck
npm run lint
cargo test --workspace
cargo clippy --workspace --all-targets
```

CI runs all of these, the window and command line builds, and the Nix package on every push and
pull request that touches the companion. When `package-lock.json` changes, `npmDepsHash` in the
repository's `flake.nix` must change with it; `nix run nixpkgs#prefetch-npm-deps -- package-lock.json`
prints the new one, and CI's Nix job fails until it matches.

The flake builds `.#companion-tauri` (the desktop app), `.#companion-cli` (the command line tool)
and `.#companion` (both together); installing them is in the [user guide](../docs/companion/index.md).

## The window

The window is [Preact](https://preactjs.com) and TypeScript, built by [Vite](https://vite.dev) into
`dist/`, which Tauri serves on the desktop and on Android. It runs in WebKitGTK on Linux and in
Android's WebView, so it is laid out for a phone's width first and follows the system's light or
dark theme ([src/styles.css](src/styles.css)).

- [src/app.tsx](src/app.tsx) holds the tabs and the screens opened over them; the route lives in the
  address's hash ([src/navigation.tsx](src/navigation.tsx)), so the web view's history and Android's
  back button step back through the screens.
- [src/state/watch-provider.tsx](src/state/watch-provider.tsx) owns the connection and gives every
  screen `useWatch()`.
- Bluetooth stays behind `WatchTransport` ([src/ble/transport.ts](src/ble/transport.ts)).
  [src/ble/tauri-transport.ts](src/ble/tauri-transport.ts) carries it to the Rust core through
  `window.battletime`, which [tauri/src/bridge.js](tauri/src/bridge.js) gives the window. Opened in a
  plain browser there is no bridge, so [src/ble/mock-transport.ts](src/ble/mock-transport.ts), a
  mock BattleTime watch and a mock wasp-os bootloader, stands in.
- Settings, backups and the store are kept in the web view's local storage
  ([src/state/storage.ts](src/state/storage.ts)), and files are chosen and saved with the web view's
  own file chooser and downloads ([src/services/device-files.ts](src/services/device-files.ts)).

Set `BATTLETIME_DEV_URL=http://localhost:5173` while `npm run dev` runs, and `npm run tauri` loads the
window from Vite's dev server, reloading it on each change.

## The command line tool

Everything the app does to a watch can be done from a terminal on Linux; its commands are in
[the command line guide](../docs/companion/command-line.md). `npm run battletime -- <command>` rebuilds
it with esbuild and runs it. It imports the same `src/` modules the screens do, and talks to BlueZ
through node-ble in [node/ble.js](node/ble.js), so each protocol exists only once. When the app is
running it goes through the app's socket instead ([cli/daemon.ts](cli/daemon.ts)), so the two share one link.

The first scan remembers the watch in `~/.config/battletime-companion/cli.json`, so later commands
need no address; `--address` and `BATTLETIME_ADDRESS` override it. The tool connects to a remembered
address without scanning. BlueZ forgets an unpaired watch soon after it last saw one, so when it
does not know the address the tool runs discovery until the watch appears, and says so if it never
does.

## The Rust core and the desktop

[core/](core) is the Rust core, with no Tauri in it, so the tool and the phone can reuse it:

- [core/src/ble.rs](core/src/ble.rs) is the Bluetooth layer, on btleplug. Every step has a timeout
  and a failed connect drops the link.
  `cargo run -p battletime-companion-core --example probe` scans, connects and reads the battery and
  the time, to check it against a real watch.
- `core/src/sync` keeps a copy of the watch ([docs/sync.md](../docs/sync.md)), and
  `core/src/plugins` runs [plugins](../docs/plugins.md) in QuickJS.
- `core/src/daemon` is the [daemon](../docs/companion/daemon.md): it owns the link, the settings,
  the plugins and the sync, reconnects to the watch, and does every background job, so the watch
  stays looked after with no window open. The window follows it through `src/services/daemon.ts`.

[tauri/](tauri) is the app around it. Desktop-only Rust sits behind `#[cfg(desktop)]`:

- [tauri/src/desktop.rs](tauri/src/desktop.rs) reads notifications, MPRIS players, Do Not Disturb
  and GeoClue over D-Bus with zbus.
- [tauri/src/notes.rs](tauri/src/notes.rs) and [tauri/src/remote.rs](tauri/src/remote.rs) are the
  note file watch and the remote buttons' shell commands.

### The computer in place of a phone

On a phone the app forwards what the phone knows; on the desktop the computer plays that part,
through [tauri/src/desktop.rs](tauri/src/desktop.rs) and
[tauri/src/desktop_events.rs](tauri/src/desktop_events.rs), which hands the daemon the same phone
events Android's bridge sends. The window's [src/services/phone/desktop.ts](src/services/phone/desktop.ts)
reads the same facts for the screens, behind the `phoneBridge` interface.

| On a phone | On Linux |
| --- | --- |
| Notifications | Desktop notifications, overheard on the session bus as apps send them |
| Now playing and media keys | Any MPRIS player: browsers, Spotify, VLC and the rest |
| Do Not Disturb | The notification server's `Inhibited` flag (KDE), or Cinnamon's, GNOME's or dunst's setting |
| Calls | None; the Calls toggle is hidden |
| The weather place | A place chosen by name, or wherever GeoClue says the computer is |
| The clock | The computer's clock |

The watch's clock is set on every connection and again whenever this device's clock jumps (set by
hand, synced, a new time zone, waking from sleep), and every six hours anyway, by the daemon's
[core/src/daemon/clock.rs](core/src/daemon/clock.rs), and by
[src/state/clock-changes.ts](src/state/clock-changes.ts) in a plain browser.

Weather from the computer's location needs GeoClue running; on NixOS that is
`services.geoclue2.enable = true`, and GeoClue knows the app as `battletime-companion`.

## The app on Android

The same app builds for Android as `com.battletime.companion.tauri`. The Android SDK, NDK and JDK live
in a shell of their own, since they come to a few gigabytes:

```bash
nix develop .#companion-android
npm run tauri-android            # a debug APK for 64-bit ARM phones
npm run tauri-android:release    # the release APK for all four processors
```

Both build the window with Vite first (Tauri's `beforeBuildCommand`). The debug build writes
`tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`; Gradle and its
dependencies download on the first build, so it needs the network then. The Android project Tauri
generated is kept in `tauri/gen/android`, with the companion's icon. The shell pins SDK 36, build
tools 36.0.0, NDK 27.1.12297006 and CMake 3.22.1, and points Gradle at the SDK's own `aapt2`,
because the one Gradle downloads does not run on NixOS. It also carries a Rust with the Android
targets, and the libclang that generates QuickJS's bindings for them.

The window, the core's Bluetooth, sync and plugins are the desktop app's; what differs:

- **Bluetooth** is btleplug's Android backend. Its Java runs in the app, and the phone bridge
  plugin starts it as the app opens. The first scan or connection asks for nearby devices
  (location before Android 12) and, on Android 13 and later, notifications for the keep-alive.
- **The phone's part** comes from [tauri-plugin-phone-bridge](tauri-plugin-phone-bridge), a Tauri
  plugin whose Kotlin answers the window directly: the notification listener, call monitor, media
  tracker and keep-alive service, the phone's location for the weather, the remote buttons'
  intents, and the SOS screen and recording (`SosActivity`, `SosRecordingService`). Its `build.rs` copies in btleplug's Java. `bridge.js` gives the window
  `window.battletime.phone` in place of the desktop's `desktop` and `remote`, and
  `src/services/phone/tauri-android.ts` turns it into the `phoneBridge`.
- **The daemon outlives the window.** There is one per process, made by whichever comes first: the
  window's Tauri setup or `KeepAliveService`, which loads the app's library and calls
  `NativeDaemon.start` with the same data folder Tauri uses (`Context.dataDir`). The daemon's device
  is `tauri/src/phone_device.rs`, whose requests reach `DaemonHost.kt` over JNI
  (`tauri/src/android.rs`); `DaemonHost` hands the plugin's phone events straight to the daemon, so
  the window never forwards them. The daemon asks the service to run while it keeps a watch, and to
  stop when it lets go; the service is `START_STICKY` and shows the `connectedDevice` notification.
- **Starting in the background.** Android 12 and later let a foreground service start from the
  background only in some cases. The window makes the watch a companion device through
  CompanionDeviceManager before connecting, while the watch still advertises; that exempts the app,
  and on Android 12 and later `WatchPresenceService` starts the service when the watch comes into
  range. `BootReceiver` starts it after a boot or an update when the daemon's
  `companion/settings.json` remembers a watch to reconnect to. Settings offers to lift the battery
  limits, which some makers' phones need before they leave a service running.
- **Files** chosen for a note or a plugin come from Android's picker as `content://` links, read
  through Tauri's fs plugin; a note is read again on each check rather than watched.
- **Plugin pages** are served at `http://plugin.localhost/<id>/`, since Android's webview reaches
  the app's own schemes that way.
- **Not on Android**: D-Bus, note file watching and shell commands for remote buttons, which are
  compiled out.

### Releases

Each `v*` tag publishes `BattleTime-Companion-<version>.apk` with the watch images and the store
([release.yml](../.github/workflows/release.yml)). It is one APK for all four phone processors,
built by `npm run tauri-android:release`, with the version taken from the tag.

Every push to `master` also builds the APK, kept as the `companion-android` artifact of its
[Companion](../.github/workflows/companion.yml) run. It is numbered one patch above the newest `v*`
tag (0.2.2 after v0.2.1), so it installs over that release with its data kept, and the next release
installs over it in turn.

It is signed with the release key, which the repository keeps in its secrets:
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_PASSWORD` and
`ANDROID_KEY_ALIAS`. The workflow writes the key into `tauri/gen/android` as `release.jks` and
`keystore.properties`, which git ignores, and deletes both after the build. Without
`keystore.properties`, a release build comes out unsigned.

Every later release must be signed with the same key, or phones refuse it as an update.

## Logs

`core/src/logging.rs` keeps one log for the core, the app and the window. It covers each step of
connecting and its failures, every GATT read, write and subscribe with its characteristic, links
dropping, the daemon bringing the watch back, sync passes and plugin lines. It also logs Rust
panics. Each time the watch connects it logs why the watch last restarted and the crash before
that, read from its diagnostics, so a restart loop shows as one such line per connection.

- **On Android** it goes to the system log under the `BattleTime` tag, and the Kotlin half uses
  `PhoneBridge`: `adb logcat -s BattleTime PhoneBridge`.
- **On the desktop** it goes to the terminal.
- **On both**, it is also written to `logs/companion.log` in the app's data folder. Past 2 MB the
  file becomes `companion.old.log` and a new one starts.

The companion's own lines are kept down to debug, btleplug's down to info, and other libraries'
warnings only. The window's `console.warn`, `console.error` and uncaught errors reach the same log
through the `app_log` command.

## Layout

```
index.html, src/main.tsx  The window's entry point, which Vite builds into dist/
src/app.tsx              The tabs and the screens opened over them
src/screens/             One file per screen
  watch.tsx              Watch: connection, status, diagnostics and quick actions
  firmware.tsx           Send a firmware package to the watch over DFU
  notifications.tsx      Forwarding toggles
  console.tsx            Raw UART traffic and a line to send as it is
  settings.tsx           App settings: weather place, hosts watch apps may fetch, GitHub builds
  scan.tsx               Lists nearby watches
  apps.tsx               Install a compiled .ntapp image, hide or remove an app, see its memory
  setup.tsx              Set up a new watch: a face, the launcher and apps
  store.tsx              Every app, face and plugin to install, and more from files and GitHub
  plugins.tsx            Installed plugins, their watch apps and their settings pages
  notes.tsx              The note the watch's Notes app shows (typed, a file or a URL), and its font
  qr-code.tsx            QR codes for text, links or Wi-Fi networks, and the list the watch's qr-code app keeps
  survival.tsx           A checklist of the emergency guides the watch's survival app carries
  remote.tsx             The watch's Remote app: its buttons, what each runs here, and recent taps
  sos.tsx                The watch's SOS: emergency contacts, whom to call, recording, and a running SOS
  launcher.tsx           The launcher's style, view and groups, and which group each app is in
  quick-apps.tsx         The apps a swipe left or right on the watch face steps through
  files.tsx              The watch's file system: browse, send, read, delete, free space
  backup.tsx             Copy the watch's settings and app state off and back
  steps.tsx              The last week of steps, by day and by hour
  alarms.tsx             The watch's alarms and the days they repeat on
  watch-settings.tsx     Brightness, timeout, units, step goal, the Gadgetbridge name and theme on the watch
src/components/          Shared building blocks: cards, rows, buttons, fields, the plugin page frame
src/styles.css           The light and dark theme and the components' look
core/                    Rust core: Bluetooth on btleplug, sync, the plugin host and web requests
tauri/                   Tauri app: the window's bridge, D-Bus, note files, remote commands, plugins;
                         gen/android holds its Android project
tauri-plugin-phone-bridge/  The phone bridge on Android: Kotlin services and commands
plugins/                 Example plugins; npm run build-plugin -- plugins/<name> builds one
src/ble/                 Transport layer: the bridge to the core, and a mock for a plain browser
src/dfu/                 Firmware updates: Nordic legacy DFU and the package reader
cli/                     The command line tool, which runs the same code as the screens
node/                    The command line tool's BlueZ over node-ble, web requests and shell commands
src/protocol/            BattleTime's GATT services, the file service, the Gadgetbridge messages,
                         backups and the requests the watch answers
src/services/            Weather, the watch apps' http proxy, GitHub builds, files on this device
src/services/phone/      Notifications, calls, media and Do Not Disturb from the phone or computer
src/state/               Watch provider (connection, capabilities, traffic, settings), routes, storage
```

## What the watch serves

The app reads which services the watch serves right after connecting, in
[src/protocol/battletime.ts](src/protocol/battletime.ts), and offers each feature only where it works:

| Service | UUID | Gives |
| --- | --- | --- |
| Legacy DFU | `00001530-1212-efde-1523-785feabcd123` | firmware updates |
| Current Time | `0x1805` / `0x2A2B`, `0x2A0F` | the clock and time zone, set on every connection |
| Battery | `0x180F` / `0x2A19` | the battery level |
| BattleTime diagnostics | `6e540001-…` / `6e540002-…` | eight 32-bit counters and the last restart with its crash; a written `01` says a debug report holds the crash |
| Device Information | `0x180A` / `0x2A26` | the firmware version, for debug reports |
| File transfer (BLEFS) | `0xFEBB` / `adaf0200-…` | files, backups, apps |
| BattleTime packages | `6e540001-…` / `6e540003-…` | installing, hiding and removing apps, the memory each needs and the watch's refusals |
| Nordic UART | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | the Gadgetbridge messages |

The BattleTime UUIDs share the base `6e54xxxx-0000-4000-8000-4e656f54696d`. The full description,
which the firmware and this app both follow, is [companion-protocol.md](../docs/companion-protocol.md) in the watch's docs.

| State | Tabs |
| --- | --- |
| No watch connected | Watch, Settings |
| BattleTime without the UART channel | Watch, Firmware, Settings |
| A watch with the UART channel | all of them |
| Waiting in a bootloader | Watch, Firmware, Settings |

A watch counts as running firmware when it serves the UART or BattleTime's own service, and as a
bootloader when it serves only DFU. BattleTime's advertisement has no room for its own 128-bit UUID,
so a scan recognises it by the name "BattleTime".

The mock watch comes as a BattleTime watch serving everything and a wasp-os watch waiting in its
bootloader, `DfuTarg (mock)`, so both states can be worked on with no hardware.

## Firmware updates

Firmware is sent as a Nordic DFU zip, the one BattleTime's `tools/package_dfu.py` makes from
`firmware.bin`, holding one application image; a zip with a SoftDevice or bootloader part is refused. BattleTime takes the update in its running firmware, at the watch's own address, so
there is no reset into a bootloader first. After the image is activated the watch restarts, the
bootloader swaps it in, and the app reconnects and writes the time. That exchange is what makes
BattleTime keep the new firmware; without it the next reset rolls back to the old one.

When the link drops partway through, the app reconnects and starts the same image again, up to
four more times. After the init packet it asks with `REPORT_RECEIVED_IMAGE_SIZE` how much the watch
kept, then sends only the rest. A refusal from the watch is not retried. A watch in an old system's
bootloader keeps nothing, so it gets one try, as before.

A watch still on wasp-os is moved over by sending it BattleTime's `reloader-from-wasp-os.zip`. The
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

Three messages are answered: `settings`, `status` and `slots` each come back with a reply of
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

## Steps and alarms

The Steps and Alarms screens and `battletime steps` and `battletime alarms` read the watch's own records
through [src/services/watch-records.ts](src/services/watch-records.ts). In the Tauri app they come
from the core's [sync](../docs/sync.md) copy, refreshed on each sync report, and changes go through
the core. Elsewhere they are read with the file service: a day's steps from its block,
`/day_blocks/steps/<n>`, laid over `/steps/<year>/<MM>-<DD>` where older firmware left that file
([src/protocol/steps.ts](src/protocol/steps.ts)), and the alarms from the
alarm app's saved state, `/apps/alarm/state`. [src/protocol/app-state.ts](src/protocol/app-state.ts)
reads and writes that `NTST` file as the core's `state_file.rs` does, changing only the values named
and keeping each one's type and width; [src/protocol/alarms.ts](src/protocol/alarms.ts) turns it into
alarms and back. After writing it the companion sends `{"t":"state","app":"alarm"}`, so the watch
takes the new alarms up. The mock watch serves a fortnight of steps and two alarms.

## Notes

The Notes screen keeps a note on the watch, for BattleTime's `notes` app to show read only. The note
comes from one of three sources, chosen on the screen and read through
[src/services/note-source.ts](src/services/note-source.ts): text typed on the screen, saved and sent
once typing pauses for a moment; a text file of the user's choice; or an http or https URL, fetched
again on every check with the headers, login and certificate choice set beside it (see
[Web requests](#web-requests)). [src/protocol/notes.ts](src/protocol/notes.ts) writes it to `/notes/note.txt` and
the chosen font's name to `/notes/font` over the file service, each only when it differs from what
was last sent; the first check on a connection lists `/notes` too, so a note deleted on the watch
is sent again. [src/state/note-sync.ts](src/state/note-sync.ts) checks when the watch connects,
every minute, and when the source, its contents or the font change here, one check at a time. Notes are limited to
64 kB.

[src/services/note-file.ts](src/services/note-file.ts) reaches a chosen file through the core
([tauri/src/notes.rs](tauri/src/notes.rs)). On the desktop it is picked in a file dialog and read
from disk, and its folder is watched, so a save is sent within a moment. On Android the picker gives
a `content://` link that is read again on each check, and the phone may withdraw the app's access
to it after a restart, in which case the screen asks for the file again. The window in a plain
browser keeps a link to the chosen file only until the page reloads.

## QR code

The QR code screen makes a code from text, a link or a Wi-Fi network (name, password, security
and whether it is hidden), shows it on the phone, and adds it to the end of the watch's `qr-code`
app's list, which holds up to eight. Each code has a caption, cut to 24 plain ASCII characters,
that names it in the list; without one the text (less a link's scheme) or the network's name is
used. Under the form the screen lists the codes on the watch, read when it connects, each with
Rename, Up, Down and Remove, and a button that removes them all.

[src/protocol/qr-code.ts](src/protocol/qr-code.ts) encodes a code with the
[qrcode](https://github.com/soldair/node-qrcode) package's encoder at error correction M unless
another level is chosen and packs it one bit a module into `/qr/<slot>`, in the first free slot
from 0 to 7. `/qr/list` holds the order and the captions, so renaming, moving or removing a code
rewrites only the list (the layout is in [companion-protocol.md](../docs/companion-protocol.md)). A
code over 120 modules is refused, and one over 74 is sent with a warning that it may not scan.
`battletime qr <text>|wifi <ssid> [password]` adds a code from a terminal, and
`battletime qr list|rename <n> <label>|move <n> up|down|remove <n>|remove all` manages the list by
the numbers `list` shows.

## Survival

The Survival screen is a checklist of the emergency guides in
[src/bundles/survival-guides.ts](src/bundles/survival-guides.ts), grouped into the topics the watch
lists them under (first aid, wilderness, disasters and personal safety), each readable on the phone first. Its switches start as the watch's `/survival/list`
says, read when it connects, and sending puts exactly the ticked guides on the watch.
[src/protocol/survival.ts](src/protocol/survival.ts) writes each guide's text to `/survival/<slot>`
unless the watch already holds it at the same length, then the list, then deletes the guides left
out (the layout is in [companion-protocol.md](../docs/companion-protocol.md)). Each guide keeps its
slot for good, and a new one takes the next free slot up to 23. `battletime survival` lists the
guides from a terminal, and `battletime survival set|add|remove <guide>...|all` changes them.

## Remote

The watch's `remote` app shows up to nine buttons and reports taps; the companion runs what each
button does. The Remote screen sets them up: each has an icon from the shared library, a label that
only the companion shows, and an action.

| Action | Where it runs | How |
| --- | --- | --- |
| Web request | everywhere | the method, headers, body, login and certificate choice given, 15 s timeout; see [Web requests](#web-requests) |
| Command | desktop app, command line | the user's shell, from the core ([tauri/src/remote.rs](tauri/src/remote.rs)) or the tool ([node/remote-command.js](node/remote-command.js)), 30 s timeout |
| Android | Android app | a link opened with `ACTION_VIEW`, an activity or a broadcast, through `tauri-plugin-phone-bridge` |
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

## Web requests

The notes URL and remote buttons make their requests through
[src/services/web-request.ts](src/services/web-request.ts), and set them up with the same fields
([src/components/web-access-fields.tsx](src/components/web-access-fields.tsx)): extra headers, a
basic-auth login, and which https certificate to trust. That is the usual check, a self-signed
certificate pinned by its SHA-256 fingerprint, or any certificate. A pin is checked during the
TLS handshake, so the headers and login never reach a server that fails it, and the handshake still
proves the server holds the certificate's key.

A window's or a phone's `fetch` cannot change how certificates are checked, so each app makes the
request natively: the app, on the desktop and on Android, through `web_request` in
[core/src/web_request.rs](core/src/web_request.rs), and the command line tool through
[node/web-request.js](node/web-request.js). Native requests also skip the window's cross-origin
rules. Only the window in a plain browser falls back to `fetch`, where a server must allow cross-origin reads
(`Access-Control-Allow-Origin`) and a choice other than the usual check is refused. Logins are kept
in the app's settings, unencrypted, as the GitHub token is. The command line takes the same choices
as `--header 'Name: value'`, `--user name:password`, `--pin <sha256>` and `--insecure`.
`node/testdata` holds the self-signed certificate and key the tests serve, made for them only.

The icon names are copied from `ICON_NAMES` in BattleTime's `compiler/src/types.rs` into
[src/bundles/icon-names.ts](src/bundles/icon-names.ts) with `npm run import-icon-names
path/to/BattleTime`; a test compares the two whenever a BattleTime checkout sits beside this one. The
picker draws each icon from the shared library image in the setup bundle
([src/protocol/library-icons.ts](src/protocol/library-icons.ts)), as the watch does.

`battletime remote list|push|run <n>|listen` does the same from a terminal, with the buttons in
`~/.config/battletime-companion/remote.json` (or `--buttons <file>`) in the form the app stores them.
In the browser's mock mode each button has a "Tap as the watch" button.

## Launcher

The watch's launcher groups the apps, and the Launcher screen arranges it. The **style** is how it
shows the groups: folders (a list of groups, each opening its apps), sections (one list with a
heading above each group), pages (a page per group, swiped left and right) or one plain list. The
**view** is how a group's apps are drawn: a row each, or a grid of icons. Groups can be renamed,
given a shared library icon, added and removed, and an app moved to another group. The **sort** puts
the groups (Other last) and each group's apps in order of their names, or, set to custom, keeps the
order the wearer gives them; switching to custom starts from the order shown, and only then can
groups and apps be moved up and down.

Each app's image declares the group it starts in, such as `games`, and the `apps` reply carries it.
[src/protocol/launcher.ts](src/protocol/launcher.ts) works out where every installed app goes, each
in the group it declares until the wearer moves it (a new version declaring another group moves an
app the wearer left alone), and writes that to `/settings/launcher` over the
file service as a few hundred bytes (the layout is in [companion-protocol.md](../docs/companion-protocol.md)), then
sends `{"c":"launcher"}` on the packages characteristic so the launcher shows it at once. Installing
an app writes the file again, so a new app lands in its group. The file lives on the watch, so
every phone and the command line see the same groups; with no file the screen starts from the
default groups: Time, Phone, Communication, Health, Games, Tools, System (settings included) and
Other. Setting up a watch writes those defaults, as folders in that order with each group's apps by name. The sort travels in a
byte after the groups that the watch ignores; a file without it reads as sorted by name. An app whose group is missing goes to Other,
and so does one installed another way, until the companion next writes the file.

`battletime launcher show|style <s>|view <v>|sort <name|custom>|move <app> <group> [place]|add <name> [icon]|rename <group> <name>|icon <group> <icon>|order <group> <place>|remove <group>|reset`
does all of the same from a terminal.

## Quick apps

A swipe left or right on BattleTime's watch face opens the next or previous app in its quick-app
ring, and a step past either end comes back to the face. The watch reads the ring from
`/settings/quick_apps` on every step: UTF-8 text, one app id a line, in ring order, with blank
lines, surrounding spaces and `\r` ignored and ids that are not installed skipped. Without the
file the ring is `steps, stopwatch, heart, weather, music`.

The Quick apps screen reads the ring when it opens, falling back to that default when the file is
missing, and lists it next to the installed apps that could join it (faces and the launcher are
left out). Apps are added, removed and moved up or down, and Save writes the file over the file
service; "Use the default" deletes it. [src/protocol/quick-apps.ts](src/protocol/quick-apps.ts)
reads and writes the file, and `battletime quick-apps [list|set <id> ...|reset]` does the same from a
terminal.

## Apps and the first run

BattleTime ships with no apps, faces or icons: a new watch shows only a help screen. When a watch with
the packages characteristic has no face, the Watch tab offers **Set up the watch**, which installs
a face, the launcher and the apps chosen, in that order. The images come from
[src/bundles/watch-apps.ts](src/bundles/watch-apps.ts), which holds every app and face in the
repository and the shared library, generated from the folder `tools/build_store.py` fills; the setup
offers only those the bundle marks for it, and the rest wait in the store. The images must match the
firmware's runtime, so rebuild the bundle when the widget or instruction numbering changes.

The shared library, with its icons, fonts and the accelerometer's configuration, ships inside the
firmware image, so a firmware update brings it and the companion never sends it. The bundle keeps
a copy only so the screens can draw the library's icons.

```bash
(cd ../battletime && nix develop --command python3 -I ../tools/build_store.py)
npm run import-watch-apps ../battletime/dist/store
```

[src/protocol/packages.ts](src/protocol/packages.ts) installs one image: an app or face is written
to `/apps/<id>/app.ntapp` and then installed with `{"c":"install","id":...}` on the packages
characteristic, which checks it on the watch; a shared library image is refused, since it comes with
the firmware. The id and kind come from the image's
own header ([src/protocol/app-image.ts](src/protocol/app-image.ts)). The Apps screen and
`battletime apps` also list, hide, show and remove apps.

## The store

The Store screen offers every bundled app and face, and the apps, faces and plugins added from files
or downloaded from a GitHub repository's releases and Actions builds, and installs or uninstalls
each on the watch. `battletime store` lists the bundle and fetches from GitHub, and `battletime apps
install <id>` installs a bundled app. [docs/app-store.md](../docs/app-store.md) describes the store,
the files a release carries and the CI that builds them.

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
| `battletime apps run <image> [--data <name>=<file>]... [--text <name>=<text>]... [--listen]` | runs the image as a temporary app with those data files (4 kB at most each), saying whether the image was sent or already there; with `--listen` it stays connected and prints each message the app sends until Ctrl+C closes the app, or stops with an error when the link drops |
| `battletime apps stop` | closes the open temporary app; its files stay |
| `battletime apps forget [id]` | closes the temporary app and deletes one temporary app's files, or all of them |
| `battletime ask "<question>" [--timeout <seconds>]` | asks with the prompt app and prints `yes` (exit 0) or `no` (exit 1); with no tap in time (60 s unless given) it closes the prompt and exits 2, and it exits 3 when it cannot ask, such as when the bundle has no `prompt.ntapp` |

## License

GPL v3 or later, like the rest of BattleTime. See [LICENSE](../LICENSE).
