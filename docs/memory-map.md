# Memory map

The PineTime has three kinds of memory: 512 KB of internal flash, 4 MB of external SPI flash and 64
KB of RAM. This page shows how NeoTime divides each one. Each box is drawn in proportion to its
size.

The flash layouts are set in code by the `neotime-boot` crate (`boot/src/lib.rs`), which the
bootloader and the firmware share, so that is the source of truth. [bootloader.md](bootloader.md)
explains how the bootloader uses them. The RAM layout is a planned budget, and each figure gets
replaced by a measurement as its part is built.

To redraw the pictures after a change, edit the sizes in `tools/draw_memory_maps.py` and run:

```sh
python3 tools/draw_memory_maps.py
```

## Internal flash (512 KB)

![Internal flash treemap](images/internal-flash.svg)

| Region | Start | Size | Holds |
|---|---|---|---|
| Bootloader | `0x00000` | 20 KB | The `embassy-boot` bootloader with its status screen and button menu, 17,752 bytes of it used. There is no SoftDevice and no MBR. |
| Slot table | `0x05000` | 8 KB | Two pages recording where the update and recovery slots sit in the external flash; see [bootloader.md](bootloader.md#the-slot-table). |
| Boot state | `0x07000` | 4 KB | The bootloader's record of an update in progress, for swapping and rolling back. |
| Firmware | `0x08000` | 388 KB | The running firmware: a 256-byte [image header](bootloader.md#the-image-header), then its code, linked to start at `0x08100`. No app, face, icon or font is built in apart from the notifications and quick settings screens (about 2.2 KB, compiled from `shell/`); [What fills the firmware](#what-fills-the-firmware) shows what is. |
| Shared library | `0x69000` | 92 KB | What apps share, up to the end of flash: images, the app host's text fonts (47,919 B), Bosch's configuration file for each accelerometer (6,144 B each, which the firmware uploads to the chip; the library with all of it is 89,218 B), components, common functions and the preset themes. No apps are kept here; they live in `/apps` on the external flash. An installed library starts with an 8-byte header, `NTLB` and its length, which `neotime-packages` writes. See [app-runtime.md](app-runtime.md#the-shared-library). |


### What fills the firmware

Measured on the release builds of `firmware/` by `tools/measure_memory.py` (see
[Measuring again](#measuring-again)). Each symbol counts towards the crate that owns it; the
firmware is linked with LTO into a single object, so the crate comes from the symbol's name, and
the files the firmware embeds, such as the Bosch configuration, are found by their bytes.

<!-- measure_memory:firmware -->
| Component | Full build | Recovery build |
|---|---:|---:|
| Firmware (start-up, shell, tasks) | 101.0 KB | 47.7 KB |
| App host (with `neotime-ui`) | 76.8 KB | 0.0 KB |
| Bluetooth (controller and TrouBLE) | 43.6 KB | 43.6 KB |
| App runtime | 34.5 KB | 0.1 KB |
| core, alloc and compiler builtins | 29.8 KB | 13.2 KB |
| Embassy, allocator and Cortex-M | 19.1 KB | 16.5 KB |
| Storage (flash store) | 17.0 KB | 16.8 KB |
| Settings, companion and notifications (with serde) | 12.4 KB | 10.2 KB |
| BLEFS file service | 9.0 KB | 9.1 KB |
| Packages | 6.1 KB | 2.0 KB |
| Display, graphics and drivers | 5.3 KB | 3.1 KB |
| Updates (DFU) | 2.1 KB | 2.1 KB |
| Other constants and padding | 28.4 KB | 13.5 KB |
| **Used** | **385.1 KB** | **177.9 KB** |
| Free of the 388 KB slot, after its header | 2.6 KB | 209.8 KB |
<!-- /measure_memory:firmware -->

The recovery build (`--features recovery`) leaves out the shell, so the app
host and the runtime go with it.

The watch face, the launcher and the apps live in `/apps` on the external flash, and the icons,
the app host's text fonts and the accelerometers' configuration files in the shared-library
region; none is inside the firmware. See
[app-runtime.md](app-runtime.md#property-values).

## External SPI flash (4 MB)

![External flash treemap](images/external-flash.svg)

The chip is an XT25F32B, on the same SPI bus as the screen. The slots are where `DEFAULT_SLOTS`
puts them; the [slot table](bootloader.md#the-slot-table) can move them anywhere below the file
system.

| Region | Start | Size | Holds |
|---|---|---|---|
| Update slot | `0x000000` | 392 KB | A new firmware received over Bluetooth, waiting for the bootloader to swap it in: the firmware's 388 KB plus one page. |
| Free | `0x062000` | 12 KB | Room the slot table can give to either slot. |
| Recovery slot | `0x065000` | 388 KB | A known-good firmware to fall back to. |
| Free | `0x0C6000` | 232 KB | Room the slot table can give to either slot, up to `0x100000`. |
| File system | `0x100000` | 3 MB | The flash store: 2 MB of files, 256 KB of records and 768 KB of logs; see [storage.md](storage.md). |

Anything that changes often, such as settings, notifications and app state, is kept here, because
the store spreads its writes across the chip. The internal flash changes only when the firmware or
the shared library is updated.

### What the file system holds

The flash store works in 4 KB sectors, one erase sector each. A file larger than 1 KB takes whole
sectors, the first beginning with a 224-byte header, and a file of 1 KB or less is a record in the
records ring, so it costs no sector. There are no folders, so an app costs nothing beyond its image.

| Path | Holds |
|---|---|
| `/apps/<id>/app.ntapp` | The compiled app or face, as `neotime-compiler` writes it. |
| `/apps/<id>/state` | What the app saved, written by the shell; a record. |
| `/apps/<id>/disabled` | Present when the app is installed but hidden from the launcher. |
| `/settings/system` | The watch-wide settings, 41 bytes. |
| `/settings/theme` | The theme's colours, 16 bytes. |
| `/settings/launcher` | The launcher's style, view and groups with their apps, which the companion writes; a few hundred bytes, kept in the system heap while the watch runs. |
| `/resources/` | For resource packages; empty for now. |
| `/steps/<year>/<month>-<day>` | One day's steps in six-minute slots, up to 480 bytes, a record; the last 90 days are kept, at most about 47 KB. |
| `/steps/trip` | The trip count, 4 bytes. |

A typical install is the digital face, the launcher and ten everyday apps (alarm, timer,
stopwatch, settings, weather, steps, heart, flashlight, music and calculator), which
`tools/measure_memory.py` installs into a store laid out as the watch's:

<!-- measure_memory:file-system -->
| What | Bytes | File sectors |
|---|---:|---:|
| `/settings/system` and `/settings/theme`, kept as records | 77 | 0 |
| 12 app images, 715 B (flashlight) to 3,919 B (settings); those of 1 KB or less are records | 22,801 | 11 |
| **In use** | | **11 of 512: 44 KB of 2 MB, and 1,960 B of records in the 256 KB records ring** |
<!-- /measure_memory:file-system -->

The rest, almost 2 MB of files and most of the records ring, is free for apps, saved state and
resources.

## RAM (64 KB, planned)

![RAM treemap](images/ram.svg)

While the bootloader runs it uses all 64 KB from `0x20000000`. Nothing is kept when it jumps to the
firmware, which then has the whole of RAM but its last 16 bytes, the alive marker
(`FIRMWARE_ALIVE_MARKER`) an emulator reads; the bootloader and the reloader keep their stacks
below it too, so the marker survives from one firmware start to the next. Its third word is the
bootloader's note to the firmware (`BOOT_NOTE_ADDRESS`), which says whether the recovery firmware
was just installed, and its fourth is the bootloader updater's (`UPDATER_NOTE_ADDRESS`), which says
the rollback after it was meant. The firmware measures 55 KB of static RAM:
31 KB of heap (10 KB for the system and the 21 KB UI heap), the flash store's tables (2.7 KB), and
the Bluetooth host with its eight 128-byte packets, controller, the display's two 1,920 B strip buffers and task state. The
Bluetooth task's state is 4.8 KB: its GATT table (2.1 KB) and TrouBLE's runner (1.6 KB) are most of
it; replies to the companion app and half-received lines wait on the heap, and the update state,
status page and confirmation are held once in a static. That leaves 8.8 KB (8,996 B) for the
stack, of which the lowest bytes, up to 360 B, are the stack guard: 256 B from the first 256-byte
boundary above the statics, which the memory protection unit forbids (`firmware/src/stack_guard.rs`).
Measured in the emulator by painting the stack before start, with 8,688 B above the guard and a
typical install, the deepest path peaks at 6,616 B, 2,072 B spare; on blank flash it is about 5.1 KB. A
store call's state, up to 1.8 KB, is on the heap while it runs, both for the shell's calls and in
the storage task, so neither the stack nor the task's static space holds it. Opening an app: the app
host loads the image and runs its handlers, and the interpreter formats numbers with 64-bit
division, all under the shell's task. Opening the launcher, the face and the alarm's editor all
peak there. Start-up reaches 3,816 B: the Bluetooth server is built straight into its static
(`third_party/trouble-host-macros`), where building it by value took a 6.9 KB frame, and the radio
and Bluetooth tasks start out of line from `main`. Drawing holds no buffer on the stack: the
display's two strip buffers, 3,840 B together, are static, and drawing's own frames stay under the
interpreter's. Earlier measurements, of the watch's own `/apps` tree (16 apps and faces) with
`apps` and `install` commands sent over the emulator's Bluetooth bridge, put packages commands
below the start path of that time. Under a connection the Bluetooth task's own frame is 0.6 KB and `serve_connection`'s
2.1 KB; packages commands run in the storage task, since the verifier inside one once overflowed
the stack under those frames, and the app host verifies an image and runs its first derives outside
the 1.7 KB frame that loads it.

The last 60 bytes of the firmware's static RAM, in the `.uninit` section just below the stack, are
the crash record (`firmware/src/restart.rs`, laid out by `neotime-crash-record`). Start-up does not
zero `.uninit`, so a panic or HardFault written there survives the reset that follows. It sits at
about `0x2000D68C`, wherever the linker puts the end of the other statics, so each build has its own
address and a record from another build fails its checksum. The bootloader runs between the two
starts but keeps clear of it: its statics are 336 B at the bottom of RAM and its stack reaches about
4.7 KB below the alive marker in an ordinary start, measured in the emulator, against the record's
10.3 KB. A stack that grew into the record would only happen after the firmware's own stack had
already overflowed.
To measure it again after a change, build the firmware and run, inside `nix develop`:

```sh
(ulimit -v 8000000; python3 -I tools/measure_stack.py firmware/target/thumbv7em-none-eabihf/release/neotime-firmware 8 --launcher)
```

It fills the stack with a pattern in the PineTime emulator, starts the firmware without the reset
its first start would cause, swipes to the launcher, and prints the peak, the spare bytes and any
resets. Spare bytes are counted down to the top of the guard. Keep at least 2 KB spare.

With apps installed and packages commands sent over Bluetooth, as the companion app sends them:

```sh
cargo run -p neotime-packages --example filesystem_use -- --no-theme --image flash.bin launcher=launcher.ntapp ...
(ulimit -v 12000000; PINETIME_RENODE_EMULATOR=<that checkout> \
    python3 -I tools/measure_stack.py firmware/target/thumbv7em-none-eabihf/release/neotime-firmware 3 \
    --flash flash.bin --library shared.ntlib --packages '{"c":"apps"}' '{"c":"install","id":"alarm"}')
```

`--packages` starts the emulator with its Bluetooth bridge and sends each line through
`tools/packages_over_hci.py`, which speaks HCI to the bridge directly, needing neither BlueZ nor
root. The Rust engine has no bridge yet, so this runs the Renode emulator it replaced, from a
PineTimeEmulator checkout pinned at 97ebb02 that `PINETIME_RENODE_EMULATOR` names, for example
`git -C ../PineTimeEmulator worktree add --detach .claude/worktrees/renode 97ebb02`. The bridge's controller now and then holds a packet back until more HCI traffic arrives, so
the client nudges it with a harmless command and sends a line again when the watch could not read it.

Static RAM by owner, from `tools/measure_memory.py`. The firmware's own share is mostly Embassy's
task storage, which holds each task's state, and the Bluetooth host's packet pool:

<!-- measure_memory:ram -->
| Owner | Full build | Recovery build |
|---|---:|---:|
| Heap (system 10 KB, UI 21 KB; the recovery build has no UI heap) | 31,744 B | 10,240 B |
| Firmware: task storage, the Bluetooth host's resources and packet pool | 21,655 B | 13,702 B |
| Bluetooth controller | 2,545 B | 2,545 B |
| Display, graphics and drivers | 372 B | 372 B |
| BLEFS file service | 122 B | 122 B |
| App host (with `neotime-ui`) | 56 B | 0 B |
| Other | 1 B | 1 B |
| **Static RAM** (`.data` and `.bss`) | **56,548 B** | **27,028 B** |
| Left for the stack | 8,972 B | 38,492 B |
<!-- /measure_memory:ram -->

The table below is the budget:

| Region | Budget | Holds | Status |
|---|---|---|---|
| UI heap | 21 KB | The heap the open app lives in. The `neotime-ui` crate keeps no screen buffer of its own. `neotime-app-host` keeps the open app's state, interpreter and bindings here (6,628 B), one 160 B node per widget of the open screen that is not inside a hidden one, each its own allocation so no large free block is needed, plus its text, and the open app's image read from `/apps`. The system's 10 KB shares the same heap. How much an app needs is worked out from its image, as [below](#how-much-heap-an-app-needs). The stopwatch demo's screen holds 1,016 B on the computer and peaks at 1,080 B while drawing. A `TextFile` holds 4 B for every eighth line of its file and the text of the lines near the view, under 2 KB in the small font, and borrows a 512 B buffer while it reads; a 2,000-line note takes about 1 KB of places. | Partly measured: node size with `size_of` for the watch, the heap peaks in the emulator, the demo screen with a counting allocator on the computer |
| Bluetooth | 12 KB | The controller and the TrouBLE host: connection state, packet buffers and the GATT tables. | Partly measured: the controller is 1.5 KB; TrouBLE's example with it is 12.0 KB in all, executor included |
| Apps | 6 KB | Memory for the app that is open beyond the UI heap, such as the parts of its file read from `/apps`. The runtime's working memory in it is measured: 2,168 B for the interpreter, 2,056 B for the bindings and 2,440 B for the state. `AppHost` keeps all three on the UI heap. | Partly measured |
| Stack | 8 KB | The main stack, for interrupts and code that is not in a task. | Planned |
| Tasks | 6 KB | Embassy's tasks. Each task's state is fixed in size when the firmware is built. | Planned |
| Reserve | 3 KB | Left free, so a budget that runs over does not crash the watch. | Planned |
| System state | 4 KB | Settings, notifications, alarms and the step count. The latest 5 notifications are kept on the heap, about 200 B each, beside about 200 B of the shell task's state. | Planned |
| Display buffer | 2 KB | The buffer `neotime-display` fills before each SPI transfer. | Planned |
| Flash store | 2.7 KB | The store's tables, in a static rather than the heap: 1 KB of record keys, 512 B of files, the rings' sector order, the log batches and an upload's state. Its calls borrow up to about 1 KB of stack. | Measured with `size_of` for the watch |

The 64 KB is all the chip has, so a part that needs more must take it from another row of this
table.

### How much heap an app needs

`neotime_runtime::memory_need::estimate_memory_need` works out an app's need from its image alone,
so the package manager can check it before an app is installed or turned on, and the shell before
it opens one. The parts, in bytes:

| Part | Bytes | Where the number comes from |
|---|---:|---|
| The image | its length | Read into the heap while the app is open |
| Working memory | 6,628 | The host's state, interpreter and bindings, with `size_of` for the watch |
| Subscriptions | 40 each | What the host keeps for each |
| Saved state | `saved_state_bytes` | The buffer the shell writes it from when the app closes, and reads it into when it opens |
| Passing use | 1,536 | Drawing, the image checks at open, and the allocator's gaps |
| Nodes | 164 per widget of the largest screen | A node and its place in the list; every widget counts as shown, a Repeat counts each row (8 when the count is worked out at run time, only those in view down a list of fixed-height rows), and no screen more than 128 |
| Text | each fixed text's length, 16 for text worked out at run time, 2,560 for a TextFile | |

An app's need must fit `APP_HEAP_BUDGET_BYTES`, 27,648 B: the 31,744 B heap less 4 KB for the
system while an app is open. At rest the system holds the shell's list of apps; the rest of its
share is for kept notifications (up to about 1 KB), a small file gathered while it uploads (up to
1 KB), and lines to and from the companion app.

Measured in the PineTime emulator on 2026-10-12 with `tools/measure_heap.py`, which installs the
`digital` face, the launcher and one app, opens each and reads the allocator's bytes in use with
its first screen open, against the estimate (which counts hidden widgets, so it is the larger):

```sh
cargo run -p neotime-packages --example filesystem_use -- --no-theme --image flash.bin \
    digital=digital.ntapp launcher=launcher.ntapp settings=settings.ntapp
python3 -I tools/measure_heap.py firmware/target/thumbv7em-none-eabihf/release/neotime-firmware flash.bin shared.ntlib
```


| App | In use | Estimate | Highest heap byte touched |
|---|---:|---:|---:|
| `digital` face | 12,072 B | 12,700 B | 13,156 B |
| `launcher`, one app listed | 7,980 B | 8,524 B | |
| `alarm` | 12,344 B | 18,224 B | |
| `settings` | 19,288 B | 19,628 B | 20,676 B |
| `weather` | 10,752 B | 16,644 B | |
| `stopwatch` | 12,016 B | 14,488 B | |

Those were measured with littlefs, whose buffers took about 1 KB of the heap, and in use counts
them. Measured again with the flash store on 2026-10-13, with a typical install: the `digital`
face 11,864 B in use, the launcher 8,156 B and the alarm 12,136 B; with the alarm's editor open the highest byte touched
was 13,188 B of the 31,744 B heap. The shell keeps a list of every installed app, turned off
or not, for the Software app: 62 B an app measured with 38 installed, against the
`LISTED_APP_BYTES` (48 B) plus id and name every app's need counts for each. With every app in
the repository and the `digital` face installed, the face held 14,356 B and the launcher, listing
every app, 11,832 B; the harness works the launcher's need out at 17,169 B with all 55 apps and
faces on, so it fits with room to spare. A launcher's need is its own, plus the names and icons of
the apps it lists, plus the largest of their images, which it reads one at a time for the icons.

The largest app in the repository, `settings`, needs 19.6 KB,
so every app fits with room to spare, and `harness/tests/quality.rs` fails any app or face whose
estimate is over the budget, or whose opening screen builds more nodes than it counts. Before the
saved-state buffer was sized for each app, closing any app borrowed 18,694 B for it, which took
the highest byte touched after leaving `settings` to 30,796 B.

The refusals were checked the same way, over the emulator's Bluetooth bridge, with every app, the
`digital` face and a 36,580 B test image uploaded to `/apps/huge` but not installed:

```sh
PINETIME_RENODE_EMULATOR=<that checkout> python3 -I tools/measure_stack.py <firmware> 5 --launcher \
    --flash flash.bin --library shared.ntlib --packages '{"c":"install","id":"huge"}' \
    '{"c":"enable","id":"launcher","on":false}' '{"c":"enable","id":"digital","on":false}' \
    '{"c":"enable","id":"timer","on":false}' '{"c":"enable","id":"timer"}' '{"c":"apps"}'
```

`huge` was refused with `"why":1,"need":36580`, the launcher and the only face with `"why":3` and
`"why":4`, the timer went off and on again with `"need":16104`, and the launcher still opened with
every app listed, the stack peaking at 7,848 B, 2,632 B spare.

## Measuring again

The firmware, RAM and file-system tables above come from one script, which writes them into
this page between the `<!-- measure_memory:... -->` markers. Run it inside `nix develop`;
`--check` only reports whether the tables are out of date:

```sh
python3 -I tools/measure_memory.py
```

CI's Memory map job (`.github/workflows/ci.yml`) runs it on every push and pull request and keeps
the measured page as an artifact. On master, when the tables change, it commits this page as
`docs(memory-map): measure <commit>`, so nobody needs to run it before landing; rebase over that
commit as over any other. The prose around the tables, and the artifact the page is published as,
still need updating by hand.

It builds the full and the recovery firmware, sorts their symbols by crate, and runs the
`filesystem_use` example of `neotime-packages`. That example also writes the whole 4 MB external
flash with `--image <file>`, for the emulator to load with
`externalFlashMemory LoadBinary @<file> 0`:

```sh
cargo run -p neotime-packages --example filesystem_use -- --image external.bin digital=firmware/apps/face.ntapp launcher=firmware/apps/launcher.ntapp
```

## Watchdog

The bootloader leaves the watchdog running when it starts the firmware, with a 5 second timeout.
It keeps counting while the chip sleeps and pauses while a debugger has it halted. The firmware
must take it over with exactly those settings, because `embassy-nrf` refuses different ones.

The firmware feeds it only while the button is up, so holding the button for about 5 seconds
resets the watch into the bootloader, and holding it 5 seconds more installs the recovery firmware.
If the firmware hangs, the watch resets, and an update that never confirms itself is rolled back.
See [bootloader.md](bootloader.md) for the details.
