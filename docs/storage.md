# Storage

NeoTime keeps its files, settings and logs in the external flash with `neotime-flash-store`
(`services/flash_store/`). [flash-store.md](flash-store.md) describes the store itself: its files,
records and logs, how it survives a power cut and what it costs. This page says what the watch
keeps there and how the firmware reaches it.

## Where it lives

The store has the `FILE_SYSTEM` region of the external flash to itself: 3 MB from `0x100000`. It
gives 2 MB to files, 256 KB to records and 768 KB to logs. The region before it holds the firmware
update and recovery slots, which the bootloader reads as raw flash; see
[bootloader.md](bootloader.md#flash-layout).

![The external flash](images/external-flash.svg)

## What the watch keeps

The store has no folders: a path's slashes only name it, and a folder is the files whose paths
start with it. The [Files app](apps/files.md) lists them that way, a folder at a time with each
file's size. Files of 1 KB or less are kept as records, so most of these cost no sector of their
own.

### Apps

| Path | Holds |
|---|---|
| `/apps/<id>/app.ntapp` | the compiled app or watch face, as `neotime-compiler` writes it |
| `/apps/<id>/state` | what the app saved, written by the shell when the app closes or the screen turns off; the alarm app's alarms and the sports app's last sessions are kept here |
| `/apps/<id>/disabled` | present when the app is installed but turned off |
| `/temp/<id>/` | a temporary app's image and data files, which the phone writes and deletes; see [companion-protocol.md](companion-protocol.md#temporary-apps-served) |

### Settings

| Path | Holds |
|---|---|
| `/settings/system` | the watch-wide settings, each one byte naming the setting and a little-endian `i32` (see [services.md](services.md)) |
| `/settings/theme` | the theme's eight RGB565 colours |
| `/settings/time_zone` | the zone a phone last set, as the two bytes of Local Time Information |
| `/settings/launcher` | the launcher's style, view and groups, which the companion writes |
| `/settings/quick_apps` | the face's ring of quick apps, one id a line, which the companion writes |

### Files the companion writes for apps

| Path | Holds |
|---|---|
| `/notes/note.txt` and `/notes/font` | the note, and the font to show it in |
| `/qr/list` and `/qr/<slot>` | the QR codes' captions, and each code as a 1-bit bitmap |
| `/remote/buttons` | the remote app's buttons |

Their formats are in [companion-protocol.md](companion-protocol.md#files-the-companion-writes).

### Logs

Each log has a file a day, holding one value for each six-minute slot from local midnight, written
as each slot ends. The last 90 days of each are kept; older days are deleted when a new day starts.
`neotime-slot-log` (`services/slot_log/`) names the files and forgets old days for all three.

| Path | Holds |
|---|---|
| `/steps/<year>/<month>-<day>` | one day's steps, a `u16` a slot, up to 480 bytes |
| `/steps/trip` | the trip count, a `u32`, saved when a day ends and when the screen goes dark |
| `/logs/heart/<year>/<month>-<day>` | one day's average heart rate a slot, a byte each, 0 for none |
| `/logs/battery/<year>/<month>-<day>` | one day's battery charge at the end of each slot, the top bit set while charging, `0xff` while the watch was off |

The battery task ends each slot: it takes the heart rate task's average for the slot and its own
charge, and the storage task writes both. The companion reads these files with the file transfer,
or copies them on each [sync](sync.md); the layout of each is in
[companion-protocol.md](companion-protocol.md#records-the-companion-reads).

## How the firmware reaches it

- **Mounting.** At start the firmware mounts the store in place, by polling the chip, so start-up's
  task never holds a copy of it. A store of another format version is refused and the status page
  says so, rather than overwriting it. Blank flash, or flash littlefs once used, mounts empty.
- **The storage task** alone awaits the store. The Bluetooth task's file transfers, the packages
  commands, the step, heart rate and battery logs and small files such as the time zone reach it
  as requests (`firmware/src/storage_requests.rs`), one at a time. Its erases and page programs
  are started and then waited on with the timer, so the CPU and other tasks run while the chip
  works.
- **The shell** calls the store directly, without awaiting, once no request is under way, so its
  reads and writes always find the store free. These calls poll the chip, resting a moment
  between polls, so they block the CPU while an erase runs. The storage task calls the store's
  `prepare_room` after every request and when the screen goes dark, which erases the sector the
  rings will need next and cleans the records ring ahead of time, so the shell's writes usually
  only program pages. Closed apps' timers run only once no store write is under way.
- **Reads by DMA.** The flash shares its SPI bus with the screen. Reads move by EasyDMA, up to
  255 bytes a piece, in the moments the display leaves the bus free, such as while the next strip
  is drawn, and stop after the current piece whenever the display asks for the bus, so they never
  slow drawing by more than one piece. The store's `file_extents` says where a part of a file lies,
  so the firmware reads it straight from the flash; a file kept as a record is read at once
  instead.
- **Reading ahead.** An app reads a file larger than its memory a part at a time:
  `file_bytes(path, length, offset)` reads from an offset, and the `prefetch_file` effect starts reading up
  to 4 KB of a file in the background, which the next `file_bytes` of that part takes from RAM.
  The diagnostics count the bytes moved by DMA, the pauses for the display and the reads served
  from a part read ahead; see [companion-protocol.md](companion-protocol.md#diagnostics-served).
- **Sleeping the chip.** While the screen is off the flash goes into deep power-down, once the
  store is idle, and wakes before its next command. The driver also wakes it at start, since a
  reset may have left it asleep.
- **The update slots** are read and written outside the store, through the same driver, which makes
  either wait for the other.

See [firmware.md](firmware.md) for how the tasks fit together.
