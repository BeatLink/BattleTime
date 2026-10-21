# The companion's daemon

The daemon is the part of the companion that keeps the watch. It lives in the Rust core, in
[`companion/core/src/daemon/`](../../companion/core/src/daemon/), and runs whether or not a window
is open. It owns:

- the Bluetooth link;
- the companion's settings;
- the [plugins](../plugins.md) and the [sync](../sync.md);
- every job the companion does in the background.

The window is one of its clients. In a plain browser (`npm run dev`) there is no daemon, and the
window does the same jobs itself against the mock watch.

## What it does

| Job | Where |
|---|---|
| Reconnects to the remembered watch whenever it drops or comes back in range | `keeper.rs`, `mod.rs` |
| Finds out what the watch serves, sets its clock, and sets it again after a jump, a zone change or six hours | `mod.rs`, `clock.rs` |
| Reads the battery and steps on the sync interval | `mod.rs` |
| Forwards the device's notifications, calls, call summaries and music | `forward.rs` |
| Answers the watch: find phone, music keys, call answers, music and navigation requests | `mod.rs` |
| Sends the weather when the watch connects and every half hour | `weather.rs` |
| Fetches web pages for watch apps, from the hosts allowed in Settings | `http_proxy.rs` |
| Keeps the watch's note matching its source | `notes.rs`, `protocol/notes.rs` |
| Keeps the remote layout on the watch and runs its button taps | `remote.rs`, `protocol/remote.rs` |
| Runs the watch's SOS: the phone's SOS screen and recording, texts and calls | `sos.rs` |

### Reconnecting

The daemon keeps the last watch connected while **Reconnect automatically** is on. When the link
drops it tries again after 2 seconds, then waits 5, 10, 20 seconds and so on, doubling up to 5
minutes, until the watch answers.

It holds back in three cases:

- after the user chooses **Disconnect**, until they connect again;
- while the window scans, since connecting stops a scan;
- while a firmware update moves the link itself.

## Settings

The daemon keeps the settings in `companion/settings.json` in the app's data folder.
[`src/state/settings.ts`](../../companion/src/state/settings.ts) still names every key and its
default.

- The window reads the settings from the daemon and sends each change as a patch.
- The first time a window meets a daemon with nothing saved, it hands over the settings it kept in
  its own storage.
- Each part of the daemon reads only its own keys, through a serde view with the same defaults as
  `settings.ts`.

## Hosts

A host starts the daemon, gives it a `Device`, and passes its events on to whoever is listening.

| Host | Device | Runs |
|---|---|---|
| The desktop app (`companion/tauri`) | the computer over D-Bus: notifications, MPRIS players, Do Not Disturb, GeoClue, shell commands | until the user quits; closing the window only hides it |
| The Android app's `KeepAliveService` | the phone over JNI (`tauri/src/phone_device.rs`, `DaemonHost.kt`): notifications, calls, media sessions, Do Not Disturb, location, intents | while a watch is kept, with a notification of its own |

### Android

There is one daemon per process. Whichever starts first makes it, the window or the service, and
the other finds it.

- The service runs while the daemon keeps a watch, and comes back after Android stops it
  (`START_STICKY`), after a reboot, and when the paired watch comes back into range (a
  `CompanionDeviceService`, on Android 12 and later).
- Connecting first pairs the watch as the phone's companion device, which exempts the app from most
  background limits.
- **Battery limits** in Settings turns off battery optimisation, for makers whose phones stop
  services anyway.

### The desktop

- Closing the window hides it while **Keep running in the background** is on, which is the default.
  The tray icon, or launching the app again, brings it back.
- **Quit**, in Settings or in the tray's menu, stops the companion.
- **Start with the computer** adds an entry to `~/.config/autostart`, which starts the app with
  `--background`: connected to the watch, with no window open.
- A second launch finds the first through a socket in `$XDG_RUNTIME_DIR` and asks it to show its
  window, so the computer never has two companions fighting over the watch.
- The [command line tool](command-line.md#sharing-the-watch-with-the-app) uses the app's link through
  `$XDG_RUNTIME_DIR/battletime-companion.sock` while the app runs. That socket takes JSON lines
  `{id, command, args}`, answers `{id, ok, value}` or `{id, ok, error}`, and pushes every event as
  `{event, payload}`; `core/src/daemon/socket.rs` lists its commands.

## Events

The daemon sends a host these events, named as the window's bridge (`tauri/src/bridge.js`) hears
them:

| Event | Carries |
|---|---|
| `ble:event` | the Bluetooth layer's own events: devices found, lines, state, mode, notifications |
| `daemon:state` | the whole state: connection, watch, capabilities, status, battery, what plays, find phone, the last error, and each job's status |
| `daemon:settings` | every saved setting, after a change |
| `daemon:sent` | each line the daemon sent the watch, for the window's console |
| `sync:report`, `plugins:ui`, `plugins:log` | as [Sync](../sync.md) and [Plugins](../plugins.md) describe them |
