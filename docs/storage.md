# Storage

NeoTime keeps its files, settings and logs in the external flash with `neotime-flash-store`
(`services/flash_store/`). [flash-store.md](flash-store.md) describes the store itself: its files,
records and logs, how it survives a power cut and what it costs. This page says what the watch
keeps there and how the firmware reaches it.

## Where it lives

The store has the `FILE_SYSTEM` region of the external flash to itself: 3 MB from `0x100000`. It
gives 2 MB to files, 256 KB to records and 768 KB to logs. The region before it holds the firmware
images, which the bootloader reads as raw flash; see [bootloader.md](bootloader.md).

## What the watch keeps

The store has no folders: a path's slashes only name it, and a folder is the files whose paths start
with it. Files of 1 KB or less are kept as records, so most of these cost no sector of their own.

| Path | Holds |
|---|---|
| `/apps/<id>/app.ntapp` | the compiled app or watch face, as `neotime-compiler` writes it |
| `/apps/<id>/state` | what the app saved, written by the shell |
| `/apps/<id>/disabled` | present when the app is installed but hidden from the launcher |
| `/settings/system` | the watch-wide settings as records (see [architecture.md](architecture.md#settings-servicessettings)) |
| `/settings/theme` | the theme's eight RGB565 colours |
| `/settings/time_zone` | the zone a phone last set |
| `/resources/shared.ntlib` | a shared library on its way into internal flash |
| `/notes/note.txt` and `/notes/font` | the note the companion app sends, and the font to show it in; see [companion-protocol.md](companion-protocol.md#notes) |
| `/steps/<year>/<month>-<day>` | one day's steps, up to 480 bytes; the last 90 days are kept |
| `/steps/trip` | the trip count |

## How the firmware reaches it

- **Mounting.** At start the firmware mounts the store in place, by polling the chip, so start-up's
  task never holds a copy of it. A store of another format version is refused and the status page
  says so, rather than overwriting it. Blank flash, or flash littlefs once used, mounts empty.
- **The storage task** alone awaits the store. The Bluetooth task's file transfers, the packages
  commands, the step log and the time zone reach it as requests, one at a time.
- **The shell** calls the store directly, without awaiting, once no request is under way, so its
  reads and writes always find the store free. These calls poll the chip, so they block the CPU
  while an erase runs, as littlefs did; every other task's calls let the CPU go meanwhile.

See [architecture.md](architecture.md#firmware-firmware) for the details.

## littlefs for the wasp-os storage rehearsal

`neotime-storage` (`services/storage/`) still puts a [littlefs](https://github.com/littlefs-project/littlefs)
file system on the flash for one build: the wasp-os storage rehearsal, which runs as a wasp-os app to
prove the flash driver and the file system on a watch before it moves to NeoTime. It uses
[littlefs-rust](https://github.com/light-player/littlefs-rust), a function-by-function Rust
translation of the C littlefs with the same on-disk format, with two fixes kept in
`third_party/littlefs-rust` (see its `NEOTIME.md`): the workspace builds `littlefs-rust-core` without
overflow checks, as C behaves, and a cap of 256 entries per metadata block that panics on file
systems written by the C littlefs is turned off. Its `ErasedBlocks` and `next_block_to_erase_ahead`
can erase free blocks ahead of littlefs, though no build uses them now.
