# Storage

NeoTime keeps its files in a [littlefs](https://github.com/littlefs-project/littlefs) file system
in the external flash. The crate is `neotime-storage`, in `services/storage/`.

## Why littlefs

- **Power cuts.** Every change is copy-on-write, so a power cut or a reset at any point leaves
  either the old file or the new one, never a broken one.
- **Wear.** NOR flash wears out after about 100,000 erases per sector. littlefs spreads writes
  across the whole region and moves busy metadata after 500 erases.
- **Proven here.** InfiniTime runs littlefs on this same chip, so its on-disk format is known to
  suit it.

## Pure Rust

NeoTime uses [littlefs-rust](https://github.com/light-player/littlefs-rust), a function-by-function
Rust translation of the C littlefs with the same on-disk format, so the whole firmware stays Rust and
needs no C compiler. The translation keeps C's raw pointers, so it is Rust but not memory-safe Rust,
and it is young: version 0.1.

Two problems turned up while testing it, and both are handled:

- **A panic on real flash.** The translation adds where C relies on a number wrapping round. In a
  debug build Rust stops there instead, the first time a blank block, which reads as all ones, is
  formatted. The workspace builds `littlefs-rust-core` without overflow checks, which is what C does
  and what release builds already did.
- **A panicking cap.** The core stops with a panic after 256 entries in one metadata block, a cap the
  C littlefs does not have. A file system written by the C littlefs, as images built on a computer
  are, exceeds it. NeoTime keeps a copy of the wrapper in `third_party/littlefs-rust` that turns the
  cap off; see its `NEOTIME.md`.

It was cross-checked against the C littlefs 2.9: each wrote file systems the other read and changed
correctly, with files up to 200 KB, folders, renames, deletes and repeated rewrites.

## Where it lives

The file system has the `FILE_SYSTEM` region of the external flash to itself: 3 MB from
`0x100000`, in 768 blocks of 4 KB. The region before it holds the firmware update slot and the
recovery firmware, which the bootloader reads as raw flash; see [bootloader.md](bootloader.md).
`FileSystemFlash` adds the region's start to every address littlefs asks for, so littlefs cannot
reach the firmware slots.

## Folders

| Folder | Holds |
|---|---|
| `/apps` | one folder per installed app or watch face, with its code and its own data |
| `/settings` | `system`, the watch-wide settings as records (see [architecture.md](architecture.md#settings-servicessettings)), and `theme`, the theme's eight RGB565 colours |
| `/resources` | fonts and pictures shared by the system and the apps |

These are made when the file system is first prepared. What goes inside each will be settled by
the app runtime and the settings service.

## Preparing at start

`prepare_file_system` takes the flash and returns the mounted file system:

1. Try to mount.
2. If littlefs finds no file system, which is what blank flash looks like, format the region and
   mount again.
3. If mounting fails any other way, such as a flash read error, hand the flash back with the error
   and change nothing. Formatting then would wipe every file because of a bus fault.
4. Make any top-level folder that is missing.

It reports whether it formatted, so the system can tell a first start from a normal one.

## Settings

| Setting | Value | Why |
|---|---|---|
| Block size | 4 KB | the flash's erase sector |
| Read and write size | 16 bytes | the flash takes any size; small keeps the caches small |
| Cache | 256 bytes | one flash program page |
| Lookahead | 32 bytes, 256 blocks | how many blocks littlefs scans at once for free space |
| Block cycles | 500 | erases before littlefs moves metadata elsewhere |

## Cost

littlefs-rust keeps the file system's buffers on the heap, which the firmware needs for the UI
anyway. Measured in the tests on a 64-bit computer, where pointers are twice the watch's size, so
the watch uses somewhat less:

- **Heap:** 944 bytes while mounted, plus 408 bytes for each open file.

## Not done yet

- Both problems above are worth reporting to littlefs-rust, so the local copy can go.
- Flash writes block the Bluetooth host, because the external flash's driver is not async yet.
