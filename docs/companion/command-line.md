# The command line

`battletime` does from a terminal what the app does, on Linux through BlueZ or the running app. It runs the same code as
the app's screens. Run it with Nix:

```bash
nix run github:BeatLink/BattleTime#companion-cli -- scan
```

`battletime help` lists every command with its options.

## Choosing the watch

```bash
battletime scan                  # list the watches in range
battletime scan --timeout 20     # look for longer
```

When the scan finds a single watch, `battletime` remembers it in
`~/.config/battletime-companion/cli.json`, so later commands need no address. `--address <mac>` or the
`BATTLETIME_ADDRESS` variable picks another. Each command connects, does its job and disconnects;
`--verbose` prints every line to and from the watch.

## Sharing the watch with the app

When the companion app is running on this computer, `battletime` goes through the app's link to the
watch instead of opening its own in BlueZ, so the two never compete for it. The app listens on a
socket at `$XDG_RUNTIME_DIR/battletime-companion.sock` (or `~/.local/share/com.battletime.companion/`
without a runtime folder), readable by this user alone; `BATTLETIME_SOCKET` names another. The tool
uses the app when that socket answers within a moment, and BlueZ otherwise. `--direct` or
`BATTLETIME_DIRECT=1` always uses BlueZ. Through the app, a command leaves the watch connected when it
ends, and a firmware update holds the app back from reconnecting while it runs.

The socket carries one JSON object per line. A request `{"id": 1, "command": "ble_write", "args":
{"text": "..."}}` is answered `{"id": 1, "ok": true, "value": ...}` or `{"id": 1, "ok": false,
"error": "..."}`, and every event the app's window hears arrives as `{"event": "ble:event",
"payload": ...}`. The commands are the window's own: `daemon_state`, `daemon_connect`,
`daemon_disconnect`, `daemon_hold`, `ble_start_scan`, `ble_stop_scan`, `ble_write`, `ble_write_bytes`,
`ble_gatt_has`, `ble_gatt_read`, `ble_gatt_write`, `ble_gatt_subscribe`, `ble_gatt_unsubscribe`,
`ble_reconnect`, `files_begin` and `files_end`. A file service turn, a hold or a scan that a
connection still has when it closes is given back. `core/src/daemon/socket.rs` answers it and
`cli/daemon.ts` speaks it.

## The watch's state

```bash
battletime status                         # battery, voltage and today's steps
battletime time                           # set the watch's clock from this computer's
battletime diag                           # diagnostic counters and why it last restarted
battletime debug [file]                   # save a debug report and clear the crash screen
battletime settings                       # print every setting
battletime settings brightness=3 theme=ocean
battletime settings gadgetbridge_name=true   # advertise as InfiniTime, for Gadgetbridge
```

## Steps and alarms

```bash
battletime steps [days]                   # steps per day
battletime alarms                         # list the alarms
battletime alarms 07:30/12345 06:00/67/off 13:00
battletime alarms none                    # remove them all
```

An alarm is `HH:MM`, then optionally `/` and the days it repeats on (1 is Monday), then optionally
`/off`. A bare time rings once. Giving alarms replaces all of them.

## Apps

```bash
battletime apps                           # what is installed, with the memory each needs
battletime apps setup                     # install a face, the launcher and the usual apps on a new watch
battletime apps install dice              # install a bundled app by id
battletime apps install timer.ntapp       # install a file
battletime apps hide dice
battletime apps show dice
battletime apps remove dice               # remove it and what it saved
battletime quick-apps                     # the apps a swipe on the face steps through
battletime quick-apps set steps stopwatch heart
battletime quick-apps reset
```

### The store

```bash
battletime store                          # the apps and faces bundled with the tool
battletime store releases                 # GitHub releases with apps or plugins, numbered
battletime store fetch BeatLink/BattleTime 1 apps   # download the newest into apps/
```

`releases` and `fetch` take another `owner/name` repository, and with `GITHUB_TOKEN` set they see
Actions builds too. The tool keeps no store of its own; install fetched apps with `apps install`,
and plugins in the app.

### The launcher

```bash
battletime launcher                       # how the launcher groups the apps
battletime launcher style folders         # folders, sections, pages or list
battletime launcher view grid             # list (a row each) or grid
battletime launcher sort custom           # or name
battletime launcher move dice games 1     # put an app in a group, at a place
battletime launcher add Work              # a new group, with an optional icon
battletime launcher rename Work Office
battletime launcher order Office 1
battletime launcher remove Office
battletime launcher reset                 # back to the default groups
```

### Temporary apps

```bash
battletime apps run app.ntapp --data words=words.txt --text title="Hello"
battletime apps run app.ntapp --listen    # print each message the app sends until Ctrl+C
battletime apps stop
battletime apps forget [id]
battletime ask "Lunch now?" --timeout 30
```

`apps run` opens an app on top of the screen without installing it, with each data file (4 kB at
most) beside it. `ask` prints `yes` (exit 0) or `no` (exit 1), exits 2 when no one answers in time
and 3 when it cannot ask. [Temporary apps](apps-and-store.md#temporary-apps) has more.

## Notes, QR codes and the remote

```bash
battletime note shopping.txt large        # a text file, in the large font
battletime note text "Milk, bread, eggs"
battletime note url https://example.org/note.txt
battletime note remove
battletime qr https://example.org --label Example
battletime qr wifi Home secret --label "Home Wi-Fi"
battletime qr list
battletime qr move 2 up
battletime qr remove all
battletime remote list                    # the buttons in ~/.config/battletime-companion/remote.json
battletime remote push                    # send their icons to the watch
battletime remote listen                  # run each button tapped on the watch
```

`note url` and the remote's web requests take `--header 'Name: value'`, `--user name:password`, and
`--pin <sha256>` or `--insecure` for the server's certificate. `remote` takes `--buttons <file>`
for another buttons file.

## Messages

```bash
battletime notify "Build finished" "All tests passed" --sender CI
battletime find on                        # ring the watch; find off stops it
battletime vibrate 2
battletime weather Kingston               # send the current weather for a place
battletime send '{"t":"vibrate","n":1}'   # any message, as JSON
battletime listen --allow example.org     # print what the watch sends, answering its apps' web requests
```

## Files and backups

```bash
battletime files ls /settings
battletime files get /notes/note.txt
battletime files put local.txt /notes/note.txt
battletime files rm /temp
battletime files df                       # free space
battletime backup watch.json
battletime restore watch.json
```

## Firmware

```bash
battletime flash firmware.zip
battletime recovery recovery.zip
battletime slots [fit]
```

[Firmware updates](firmware-updates.md#from-the-command-line) says what each does.
