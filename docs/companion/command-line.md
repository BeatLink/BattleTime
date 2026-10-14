# The command line

`neotime` does from a terminal what the app does, on Linux through BlueZ. It runs the same code as
the app's screens. Run it with Nix:

```bash
nix run github:BeatLink/NeoTime#companion-cli -- scan
```

`neotime help` lists every command with its options.

## Choosing the watch

```bash
neotime scan                  # list the watches in range
neotime scan --timeout 20     # look for longer
```

When the scan finds a single watch, `neotime` remembers it in
`~/.config/neotime-companion/cli.json`, so later commands need no address. `--address <mac>` or the
`NEOTIME_ADDRESS` variable picks another. Each command connects, does its job and disconnects;
`--verbose` prints every line to and from the watch.

## The watch's state

```bash
neotime status                         # battery, voltage and today's steps
neotime time                           # set the watch's clock from this computer's
neotime diag                           # diagnostic counters and why it last restarted
neotime debug [file]                   # save a debug report and clear the crash screen
neotime settings                       # print every setting
neotime settings brightness=3 theme=ocean
neotime settings gadgetbridge_name=true   # advertise as InfiniTime, for Gadgetbridge
```

## Steps and alarms

```bash
neotime steps [days]                   # steps per day
neotime alarms                         # list the alarms
neotime alarms 07:30/12345 06:00/67/off 13:00
neotime alarms none                    # remove them all
```

An alarm is `HH:MM`, then optionally `/` and the days it repeats on (1 is Monday), then optionally
`/off`. A bare time rings once. Giving alarms replaces all of them.

## Apps

```bash
neotime apps                           # what is installed, with the memory each needs
neotime apps setup                     # install a face, the launcher and the usual apps on a new watch
neotime apps install dice              # install a bundled app by id
neotime apps install timer.ntapp       # install a file
neotime apps hide dice
neotime apps show dice
neotime apps remove dice               # remove it and what it saved
neotime quick-apps                     # the apps a swipe on the face steps through
neotime quick-apps set steps stopwatch heart
neotime quick-apps reset
```

### The store

```bash
neotime store                          # the apps and faces bundled with the tool
neotime store releases                 # GitHub releases with apps or plugins, numbered
neotime store fetch BeatLink/NeoTime 1 apps   # download the newest into apps/
```

`releases` and `fetch` take another `owner/name` repository, and with `GITHUB_TOKEN` set they see
Actions builds too. The tool keeps no store of its own; install fetched apps with `apps install`,
and plugins in the app.

### The launcher

```bash
neotime launcher                       # how the launcher groups the apps
neotime launcher style folders         # folders, sections, pages or list
neotime launcher view grid             # list (a row each) or grid
neotime launcher sort custom           # or name
neotime launcher move dice games 1     # put an app in a group, at a place
neotime launcher add Work              # a new group, with an optional icon
neotime launcher rename Work Office
neotime launcher order Office 1
neotime launcher remove Office
neotime launcher reset                 # back to the default groups
```

### Temporary apps

```bash
neotime apps run app.ntapp --data words=words.txt --text title="Hello"
neotime apps run app.ntapp --listen    # print each message the app sends until Ctrl+C
neotime apps stop
neotime apps forget [id]
neotime ask "Lunch now?" --timeout 30
```

`apps run` opens an app on top of the screen without installing it, with each data file (4 kB at
most) beside it. `ask` prints `yes` (exit 0) or `no` (exit 1), exits 2 when no one answers in time
and 3 when it cannot ask. [Temporary apps](apps-and-store.md#temporary-apps) has more.

## Notes, QR codes and the remote

```bash
neotime note shopping.txt large        # a text file, in the large font
neotime note text "Milk, bread, eggs"
neotime note url https://example.org/note.txt
neotime note remove
neotime qr https://example.org --label Example
neotime qr wifi Home secret --label "Home Wi-Fi"
neotime qr list
neotime qr move 2 up
neotime qr remove all
neotime remote list                    # the buttons in ~/.config/neotime-companion/remote.json
neotime remote push                    # send their icons to the watch
neotime remote listen                  # run each button tapped on the watch
```

`note url` and the remote's web requests take `--header 'Name: value'`, `--user name:password`, and
`--pin <sha256>` or `--insecure` for the server's certificate. `remote` takes `--buttons <file>`
for another buttons file.

## Messages

```bash
neotime notify "Build finished" "All tests passed" --sender CI
neotime find on                        # ring the watch; find off stops it
neotime vibrate 2
neotime weather Kingston               # send the current weather for a place
neotime send '{"t":"vibrate","n":1}'   # any message, as JSON
neotime listen --allow example.org     # print what the watch sends, answering its apps' web requests
```

## Files and backups

```bash
neotime files ls /settings
neotime files get /notes/note.txt
neotime files put local.txt /notes/note.txt
neotime files rm /temp
neotime files df                       # free space
neotime backup watch.json
neotime restore watch.json
```

## Firmware

```bash
neotime flash firmware.zip
neotime recovery recovery.zip
neotime slots [fit]
```

[Firmware updates](firmware-updates.md#from-the-command-line) says what each does.
