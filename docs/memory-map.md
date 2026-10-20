# Memory map

The PineTime has three kinds of memory: 512 KB of internal flash, 4 MB of external SPI flash and 64
KB of RAM. This page shows how BattleTime divides each one. Each box is drawn in proportion to its
size. The [memory treemap](memory-treemap.html) shows all three at once, as one map you can zoom
into, from each memory down to what fills its regions.

The flash layouts are set in code by the `battletime-boot` crate (`boot/src/lib.rs`), which the
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
| Bootloader | `0x00000` | 20 KB | The `embassy-boot` bootloader with its status screen and button menu, 18,576 bytes of it used. There is no SoftDevice and no MBR. |
| Slot table | `0x05000` | 8 KB | Two pages recording where the update and recovery slots sit in the external flash; see [bootloader.md](bootloader.md#the-slot-table). |
| Boot state | `0x07000` | 4 KB | The bootloader's record of an update in progress, for swapping and rolling back. |
| Firmware | `0x08000` | 480 KB | The running firmware, to the end of flash: a 256-byte [image header](bootloader.md#the-image-header), then its code, linked to start at `0x08100`, then the shared library. No app, face, icon or font is built into the code apart from the notifications and quick settings screens (about 2.2 KB, compiled from `shell/`); [What fills the firmware](#what-fills-the-firmware) shows what is. The code and the library share the region, so either can grow into the other's room. |

The shared library sits right after the code, on a word boundary, where the image header says. It
holds what apps share: images, the app host's text font (4,945 B, one font of lines and arcs that
every text size scales, which the full firmware's status page draws with too), components, common
functions and the preset themes; the library is 37,159 B. Bosch's configuration file for each
accelerometer (6,144 B each) lives on the external flash instead, under `/system/accelerometer`. It arrives with each firmware
update and is never written on the watch. No apps are kept in internal flash; they live in `/apps`
on the external flash. See [app-runtime.md](app-runtime.md#the-shared-library).


### What fills the firmware

Measured on the release builds of `firmware/` by `tools/measure_memory.py` (see
[Measuring again](#measuring-again)). Each symbol counts towards the crate that owns it; the
firmware is linked with LTO into a single object, so the crate comes from the symbol's name, and
the files the firmware embeds, such as the Bosch configuration, are found by their bytes. A few crates
off the drawing and interpreter paths, such as `battletime-companion` and `battletime-packages`, are built
for size (`opt-level = "z"`) in `firmware/Cargo.toml`, while the rest stay at `"s"`.

<!-- measure_memory:firmware -->
| Component | Full build | Recovery build |
|---|---:|---:|
| Firmware (start-up, shell, tasks) | 142.8 KB | 49.1 KB |
| App host (with `battletime-ui`) | 93.2 KB | 0.0 KB |
| App runtime | 34.3 KB | 0.0 KB |
| Bluetooth (controller and TrouBLE) | 31.7 KB | 41.3 KB |
| core, alloc and compiler builtins | 28.8 KB | 16.5 KB |
| Settings, companion and notifications (with serde) | 17.7 KB | 2.4 KB |
| Storage (flash store) | 17.4 KB | 13.7 KB |
| BLEFS file service | 9.5 KB | 3.1 KB |
| Display, graphics and drivers | 8.4 KB | 3.3 KB |
| Embassy, allocator and Cortex-M | 7.9 KB | 5.0 KB |
| Packages | 7.8 KB | 1.3 KB |
| Updates (DFU) | 3.3 KB | 2.8 KB |
| Other constants and padding | 26.3 KB | 14.2 KB |
| **Code and data** | **429.0 KB** | **152.8 KB** |
| Shared library, after the code | 6.6 KB | none |
| Free of the 480 KB region, after its header | 44.1 KB | 327.0 KB |
<!-- /measure_memory:firmware -->

The recovery build (`--features recovery`) leaves out the shell, so the app
host and the runtime go with it.

The heart rate and battery logs (`battletime-slot-log`, written by the battery task through the
storage task) added 1,528 B of code to the full build, measured with `cargo size` against master on
2026-10-14 (433,200 B to 434,728 B of text), and 136 B of static RAM, most of it the battery task's
future holding the storage request while it waits to send it. The step log takes its paths and its
forgetting of old days from the same crate.

Keeping those logs' days in blocks programmed a slot at a time (`program_file` in the flash store,
used by `battletime-slot-log`) added 1,432 B to the firmware image, measured against master on
2026-10-15 (488,009 B to 489,441 B with the library), and no static RAM. It cut the erases the
three logs cost from about 80 a day, when each slot rewrote its whole day as a record, to one for
each new block, about one every four days.

Capitalising text with a Latin-1 table of its own instead of `char::to_uppercase`, and making the interpreter's
empty constant all zeros so a new one is filled rather than copied from flash, saved 3,408 B of code and
constants, measured with `cargo size` against master on 2026-10-15 (439,076 B to 435,668 B of text and rodata).

Moving the 2048 slides and the Game of Life step out of the runtime and into those two games' own bytecode
saved 2,304 B of code and constants, measured with `cargo size` against master on 2026-10-15 (435,464 B to
433,160 B of text and rodata). The games' images grew instead, on the external flash: `twos` from 1,332 B to
2,460 B and `game-of-life` from 2,752 B to 3,014 B.

Moving the `dot_matrix` font out of the firmware saved 3,444 B of code and constants, measured with
`cargo size` against master on 2026-10-15 (435,580 B to 432,136 B of text and rodata), for 24 B more
static RAM. Its 5 by 7 dot table and the code that drew it are gone: the font is now a 1,414 B file
in the text font's format, made of rows of dots that the text font's own drawing code draws, and
each app that uses it carries it in its image on the external flash. It takes RAM only while that
app is loaded, as part of its image; only the hidden `casio` face uses it.

Drawing the full firmware's status page, help screen and crash pages in the shared library's text
font, in place of the 10x20 bitmap font (2,400 B), saved 1,956 B of code and constants, measured
with `cargo size` against the commit before on 2026-10-15 (432,136 B to 430,180 B of text and rodata),
for 8 B more static RAM. The library's font was already in the image, read in place, so the page
costs no RAM beyond the glyph it places on the stack while drawing. The recovery firmware has no
library and keeps the bitmap font, as do the bootloader and its updater. Starting with the shared
library and erased external flash, which shows the status page and then the help screen in that
font, peaked at 5,316 B of stack, 4,140 B spare.

Open files, which keep a file's map of flash runs so the shell reads any part of it in one DMA read
with no look-up (`Files::open_now`, `FlashStore::file_map`), added 1,992 B of code and 8 B of
constants, measured with `cargo size` against master e30358e4 on 2026-10-15 (432,020 B to 434,020 B of
text and rodata), and 16 B of static RAM. The shell keeps the file it last read open, a boxed few
dozen bytes plus a heap list of the file's runs, until the app closes. Notes reading a 6.8 KB note
from the launcher peaked at 6,332 B of stack, 3,124 B spare, against master's 6,188 B and 3,268 B.
Awaiting store reads instead of sleeping in WFE was measured too, at 4,600 B more code, and left out.

Moving the accelerometers' configuration files out of the shared library onto the external flash
shrank the library from 49,467 B to 37,159 B. Loading the detected chip's file through the storage
task added 1,068 B of code and 56 B of constants, measured with `cargo size` against the commit
before on 2026-10-15 (434,724 B to 435,848 B of text and rodata), and 32 B of static RAM, so the
firmware region gained 11,184 B. The file takes 6 KB of heap while it is uploaded, once at start.

Moving the shared library's 72 icons out of it into `/system/icons` on the external flash, read
for each screen before it paints, shrank the library from 37,159 B to 6,731 B. Reading them added
2,960 B of code, measured with `cargo size` against the commit before on 2026-10-16 (435,996 B to
438,956 B of text and rodata), and 16 B of static RAM for the icons file's own open file, so the
firmware region gained 27,468 B. A screen holds its icons on the heap, 1,040 B for each one an app
may show counted in its [need](#how-much-heap-an-app-needs), up to 8 KB. Opening the launcher's
first app with the digital face, the launcher, settings and the alarm installed peaked at 6,284 B
of stack, 2,916 B spare.

Paging the code of apps that need less of the heap that way (`runtime/src/paging.rs`) added
3,944 B of code, measured with `cargo size` against the commit before on 2026-10-16 (438,956 B to
442,900 B of text and rodata), and 32 B of static RAM for the paged app's image file's open file
and the image's two paged offsets. Seven of the repository's 66 apps and faces are paged: `fancy`
needs 18,864 B instead of 21,888 B, `omega-time` 11,036 B instead of 14,040 B, and `phrase-clock`,
`infineat`, `about`, `four-in-a-row` and `twos` from 1,376 B down to 40 B less. Opening `about`
paged and turning to its second page peaked at about the launcher's 6,252 B of stack, 2,948 B
spare, against 6,092 B for `about` whole.

Paging widget records too, so a paged app holds only the open screen's, added 2,636 B of code,
measured with `cargo size` against the commit before on 2026-10-16 (442,900 B to 445,536 B of text
and rodata), and saved 208 B of static RAM, since the host no longer keeps a copy of its image in a
widget tree. Ten apps and faces are paged now: `settings` needs 18,024 B instead of 19,232 B and
`about` 16,800 B instead of 18,412 B, while `fancy` and `omega-time`, one screen each, save about
50 B less than with code alone. Opening `settings` paged and turning on Always on peaked at
6,332 B of stack, 3,124 B spare.

With paging, measured with `tools/measure_heap.py` on 2026-10-16: the face held 6,552 B, the
launcher 4,272 B, `settings` paged 15,768 B in use with the heap touched to 16,504 B of 30,976 B
(its need is worked out at 18,024 B), and `about` paged 8,928 B, touched to 11,628 B (16,800 B
worked out). Across the 66 apps and faces, nodes are the largest part of the worked-out needs
(158 KB together), then the library icons a screen may show (168 KB, counted at 1,040 B each
though an icon averages 420 B), working memory (210 KB) and images (119 KB).

Counting each library icon an app names at its own size rather than the most an icon may take
added 1,096 B of code with the change before it, which counts an image a value picks as a library
icon only in an app that can name one, measured with `cargo size` on 2026-10-16 (445,536 B to
446,632 B of text and rodata), 144 B of it the table of sizes. The icons counted across the 66 apps
and faces fell from 168 KB to 95 KB: `fancy` needs 15,464 B instead of 18,912 B, `pinetime-style`
15,344 B instead of 18,660 B and `settings` 17,424 B. The largest need is still the stopwatch's,
20,084 B, nearly all of it nodes.

The worked-out need had left out the 40-byte extra part the host keeps on the heap for a node of
some kinds, such as an `Icon`, `Button` or `Repeat`; it now counts it, from
`memory_need::holds_extra`, which a test in the app host keeps equal to the kinds it gives one.
That added 104 B of code (446,632 B to 446,736 B) and raised the nodes counted across the 66 apps
and faces from 158 KB to 169 KB; the stopwatch needs 20,284 B, still the most.

Keeping four free file sectors erased ahead, so a file of up to four sectors is written without
waiting 45 ms an erase, added 704 B of code, measured with `cargo size` against the commit before on
2026-10-15 (434,020 B to 434,724 B of text and rodata), and 64 B of static RAM for the map of which
free sectors are erased.

Queuing the shell's store writes for the storage task and writing the update and recovery slots
through `SlotFlash`, so neither spins on the flash chip, added 4,048 B to the firmware's code image,
measured with `cargo objcopy` against master on 2026-10-15 (428,660 B to 432,708 B), leaving 9,089 B
of the region free beside the 49,467 B library, and 120 B of static RAM, mostly the Bluetooth task's
future now holding the slot writes' waits. The recovery build grew by 1,172 B of text and 124 B of
static RAM. About half the code is the queue and the storage task's handling of it; the rest is the
waits in the update receivers and the recovery move. The stack did not move: opening the launcher's
first app with the launcher, settings, stopwatch and digital face installed peaked at 6,092 B,
3,364 B spare (6,084 B before). The queue holds each waiting file once on the heap, usually for
only a moment.

Running closed apps (`firmware/src/closed_apps.rs` and `battletime_app_host::closed_apps`) added
5,008 B of code to the full build, measured with `cargo size` against master on 2026-10-14
(405,496 B to 410,504 B of text), and 32 B of static RAM. A first version wrapped the shell's
system in a second `System` type, which built the host's handler path twice and cost 23.8 KB, so
the shell's own `WatchSystem` takes the closed app's effects through `ClosedRun::hold_back`.
On the heap the shell keeps about 20 B for each closed app with a time to come (its id, due and
record times and run size). A run holds the app's image, its state, interpreter and bindings and
its saved state, plus what drawing borrows, `closed_run_bytes`: about 11.5 KB for the alarm, whose
image is 3 KB. It happens beside the open app when the free heap has that plus 2 KB, and otherwise
after closing it. The host is loaded into its slot out of line (`open_in_background_into`), as
opening an app does: loading it by value put the load's frame under the first derives, and the
start-up search for closed apps then peaked at 8,784 B of stack, 1,440 B spare.

Measured in the emulator on 2026-10-14 with the typical install (the `digital` face, the launcher
and `TYPICAL_APPS`): open the timer from the launcher, start its 10 minutes, leave it, and let it
ring while closed. The stack pattern was painted before start, and `restart`'s least free heap and
allocation failure count were read at the end:

| Timer rings over | Stack peak | Spare above the guard | Least free heap | Highest heap byte touched | Resets |
|---|---:|---:|---:|---:|---:|
| The face, screen dark | 6,352 B | 3,872 B | 8,996 B | 22,900 B | none |
| The stopwatch (11,424 B in use), run beside it | 6,352 B | 3,872 B | 9,080 B | 22,644 B | none |
| Settings (19,200 B in use), closed first and opened again | 6,352 B | 3,872 B | 10,976 B | 20,660 B | none |

Each time the timer opened on its ringing screen and lit the screen, with no allocation refused.
The same install without the timer started peaks at 5,984 B, as master does. With a 1 KB margin
the run beside Settings went ahead, leaving 1,304 B free at its lowest and touching the heap to
30,812 B of 30,976 B, so the margin is 2 KB.

The watch face, the launcher, the apps, the accelerometers' configuration files and the shared
library's icons live on the external flash, and the app host's text font in the shared library
after the firmware's code; none is inside the code. See
[app-runtime.md](app-runtime.md#property-values).

## External SPI flash (4 MB)

![External flash treemap](images/external-flash.svg)

The chip is an XT25F32B, on the same SPI bus as the screen. The slots are where `DEFAULT_SLOTS`
puts them; the [slot table](bootloader.md#the-slot-table) can move them anywhere below the file
system.

| Region | Start | Size | Holds |
|---|---|---|---|
| Update slot | `0x000000` | 484 KB | A new firmware received over Bluetooth, waiting for the bootloader to swap it in: the firmware's 480 KB plus one page. |
| Free | `0x079000` | 308 KB | Room the slot table can give to either slot. |
| Recovery slot | `0x0C6000` | 232 KB | A known-good firmware to fall back to, up to `0x100000`. |
| File system | `0x100000` | 3 MB | The flash store: 2 MB of files, 256 KB of records and 768 KB of logs; see [storage.md](storage.md). |

Anything that changes often, such as settings, notifications and app state, is kept here, because
the store spreads its writes across the chip. The internal flash changes only when the firmware,
with the shared library inside it, is updated.

### What the file system holds

The flash store works in 4 KB sectors, one erase sector each. A file larger than 1 KB takes whole
sectors, the first beginning with a 224-byte header, and a file of 1 KB or less is a record in the
records ring, so it costs no sector. There are no folders, so an app costs nothing beyond its image.

| Path | Holds |
|---|---|
| `/apps/<id>/app.ntapp` | The compiled app or face, as `battletime-compiler` writes it. |
| `/apps/<id>/state` | What the app saved, written by the shell; a record. |
| `/apps/<id>/disabled` | Present when the app is installed but hidden from the launcher. |
| `/settings/system` | The watch-wide settings, 41 bytes. |
| `/settings/theme` | The theme's colours, 16 bytes. |
| `/settings/launcher` | The launcher's style, view and groups with their apps, which the companion writes; a few hundred bytes, kept in the system heap while the watch runs. |
| `/system/accelerometer/bma421`, `/system/accelerometer/bma425` | Bosch's configuration for each accelerometer chip, 6,144 B and 2 sectors each, which the companion puts on the watch. |
| `/system/icons` | The shared library's 72 icons, 30,436 B in 8 sectors, which the companion puts on the watch; see [app-runtime.md](app-runtime.md#the-shared-library). |
| `/resources/` | For resource packages; empty for now. |
| `/day_blocks/steps/<n>` | 8 days of steps in six-minute slots, a blank file of one sector programmed a slot at a time; 90 to 97 days are kept, 12 or 13 sectors, about 52 KB. |
| `/steps/trip` | The trip count, 4 bytes. |
| `/day_blocks/heart/<n>` | 16 days of average heart rate in six-minute slots, a byte each, one sector; 90 to 105 days kept, 6 or 7 sectors. |
| `/day_blocks/battery/<n>` | 16 days of battery charge at the end of each six-minute slot, with the top bit set while charging, one sector; 90 to 105 days kept, 6 or 7 sectors. |
| `/steps/<year>/<month>-<day>`, `/logs/heart/...`, `/logs/battery/...` | Days older firmware kept as records, up to 480 bytes each and a record key each; kept until they are 90 days old. |

A typical install is the digital face, the launcher and ten everyday apps (alarm, timer,
stopwatch, settings, weather, steps, heart, flashlight, music and calculator), which
`tools/measure_memory.py` installs into a store laid out as the watch's:

<!-- measure_memory:file-system -->
| What | Bytes | File sectors |
|---|---:|---:|
| `/settings/system` and `/settings/theme`, kept as records | 117 | 0 |
| `/system/`: the accelerometers' configuration and the shared library's icons, as the companion puts them | 42,724 | 12 |
| 12 app images, 739 B (flashlight) to 7,233 B (settings); those of 1 KB or less are records | 26,383 | 12 |
| **In use** | | **24 of 512: 96 KB of 2 MB, and 968 B of records in the 256 KB records ring** |
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
the rollback after it was meant. The firmware measures 53.4 KB of static RAM:
30.25 KB of heap (10 KB for the system and the 20.25 KB UI heap), the flash store's tables (2.7 KB, the last file header read included), and
the Bluetooth host with its eight 128-byte packets, controller, the display's two 1,920 B strip buffers and task state.
TrouBLE's host keeps one connection, no L2CAP channels of its own (every service speaks over GATT), and a send
queue of four packets, set by `TROUBLE_HOST_L2CAP_TX_QUEUE_SIZE` in `firmware/.cargo/config.toml`; its receive queue
stays at eight, since a full one drops the phone's writes. The
Bluetooth task's state is 3.2 KB, TrouBLE's runner (1.6 KB) much of it; of a write it keeps only the 20 bytes an
update needs across its awaits, and a file service command goes to the storage task on the heap, so the request
channel's two places no longer hold a 128-byte packet each; replies to the companion
app and half-received lines wait on the heap, and the update state, status page and confirmation
are held once in a static. The GATT server (`SERVER`) is 656 B, mostly TrouBLE's CCCD table for
each client. Its attribute table, 61 attributes of 32 B, is a constant in flash (1,952 B), built
when the firmware is compiled by the patched macros in `third_party/`; only the 388 B of values
that are written or set at run time are in RAM, in one static byte array (`__SERVER_STORED_VALUES`).
The Heart Rate service, the last one added, took 1,076 B of flash in the full build (about 0.8 KB in
the recovery build, which serves it too), 128 B of it its four attributes
and most of the rest notifying it, and 48 B of static RAM: 28 B in the CCCD table, 4 B of value and the reading's signal.
The client for the phone's own Current Time and Alert Notification services
(`firmware/src/phone_services.rs` and `services/phone_client`) took 3,008 B of flash in the full
build, measured with `cargo size` against master fcdff6dc (433,212 B to 436,220 B of text), and
56 B of static RAM, most of it in the Bluetooth task's state; the recovery build leaves it out.
While a phone is connected it keeps 368 B on the heap, and its 268 B frame is out of line, so the
launcher's stack peak on blank flash stayed at about 6.0 KB with 2,964 B spare. TrouBLE's own
`GattClient` took 12.6 KB of flash for the same work.
Built at start-up, as upstream TrouBLE does, the table took 2,536 B with its values' stores
(2,771 B in all) and 4.4 KB of code to build it. That leaves 10.6 KB (10,828 B) for the
stack, of which the lowest bytes, up to 360 B, are the stack guard: 256 B from the first 256-byte
boundary above the statics, which the memory protection unit forbids (`firmware/src/stack_guard.rs`).
Measured in the emulator by painting the stack before start, with a typical install, the deepest
path peaked at 6,616 B with 8,688 B above the guard; with the GATT table in flash there are 10,480
B above it, which leaves that path about 3,860 B spare. On 2026-10-14 opening the launcher's first
app peaked at 6,008 B, 4,472 B spare, against 6,016 B and 2,416 B spare before the table moved; on blank flash it is about 5.1 KB. A
store call's state, up to 1.8 KB, is on the heap while it runs, both for the shell's calls and in
the storage task, so neither the stack nor the task's static space holds it. Opening an app: the app
host loads the image and runs its handlers, and the interpreter formats numbers with 64-bit
division, all under the shell's task. Opening the launcher, the face and the alarm's editor all
peak there. Opening Remote with nine buttons, whose handler reads `/remote/buttons` from the flash
store under the interpreter, peaks at 6,620 B, 2,324 B above the guard. That fits because the
text instructions that copy a text, closing the old app, and the shell loop's other branches run
out of line, so their locals are not in the frames that stay on the stack while a handler runs. Start-up reaches 3,816 B: the Bluetooth server is built straight into its static
(`third_party/trouble-host-macros`), where building it by value took a 6.9 KB frame, and its
attribute table is a constant in flash, and the radio
and Bluetooth tasks start out of line from `main`. Drawing holds no buffer on the stack: the
display's two strip buffers, 3,840 B together, are static, and drawing's own frames stay under the
interpreter's. Earlier measurements, of the watch's own `/apps` tree (16 apps and faces) with
`apps` and `install` commands sent over the emulator's Bluetooth bridge, put packages commands
below the start path of that time. Under a connection the Bluetooth task's own frame is 0.6 KB and `serve_connection`'s
2.1 KB; packages commands run in the storage task, since the verifier inside one once overflowed
the stack under those frames, and the app host verifies an image and runs its first derives outside
the 1.7 KB frame that loads it.

The last 88 bytes of the firmware's static RAM, in the `.uninit` section just below the stack, are
the kept clock (`firmware/src/clock.rs`, 20 B), the crash record (`firmware/src/restart.rs`, laid
out by `battletime-crash-record`) and the 8-byte stack overflow count, kept as the count and its bits
flipped. Start-up does not zero `.uninit`, so a panic or HardFault written there survives the reset
that follows. The kept clock is five words, which `battletime-current-time` writes and checks: "TIME",
UTC in milliseconds as two halves, the time zone and a check over the four. The watchdog task
writes it twice a second and start-up takes it back, so after a soft or watchdog reset the clock
counts on from it, and RAM after a power cut fails the check. A restored clock counts as set, the
same as one a phone set: BattleTime has no separate "set by phone" state, and the step log and alarms
go by the clock whatever set it, so a reset no longer throws them back to the build date. A kept
time earlier than the build time is moved up to it. It all sits at about `0x2000D61C`, wherever the
linker puts the end of the other statics, so each build has its own address and a record from
another build fails its checksum; the overflow count starts again from 0 the same way, and the
first start of a new firmware starts the clock from its build time until the phone sets it. The bootloader runs between the two
starts but keeps clear of it: its statics are 336 B at the bottom of RAM and its stack reaches about
4.7 KB below the alive marker in an ordinary start, measured in the emulator, against the record's
10.3 KB. A stack that grew into the record would only happen after the firmware's own stack had
already overflowed.
Advertising like InfiniTime, the `gadgetbridge_name` setting, Gadgetbridge's `nav` message, the
`request` lines and rounding fractions in companion lines took 2,816 B of flash (2,808 B of text and
8 B of data) and 48 B of static RAM in the full build, against `cargo size` of master 5eaf23f2, and
312 B in the recovery build. Advertising itself saved about 450 B, since the advertisement is a
constant rather than encoded at start; most of the cost is the `nav` reader and serde's
deserializers, which the larger line parser no longer inlines. A line with a fraction is copied
once on the heap while it is read.
The wake gestures (raise, lower and shake to wake, sleep mode and the face's double tap) took 3,808 B
of flash, 16 B of data and 312 B of static RAM, against `cargo size` of master 97551242: the wake
gesture detector, about 130 B, lives in the motion task's state and the double tap finder in the
shell's. With them, a typical install with the settings app opened from the launcher peaked at
6,132 B, 3,836 B spare. The companion's longest line went from 512 to 640 bytes for the longer
settings report, which is built on the shell's stack only while it is sent.

To measure it again after a change, build the firmware and run, inside `nix develop`:

```sh
(ulimit -v 8000000; python3 -I tools/measure_stack.py firmware/target/thumbv7em-none-eabihf/release/battletime-firmware 8 --launcher)
```

It fills the stack with a pattern in the PineTime emulator, starts the firmware without the reset
its first start would cause, swipes to the launcher, and prints the peak, the spare bytes and any
resets. Spare bytes are counted down to the top of the guard. Keep at least 2 KB spare. With
`--flash` and `--library` for an install, `--open-first-app` also taps the launcher's first row.
The UI heap gave up 768 B for the heart rate work, so the launcher keeps as much stack spare as
without it.
`--then` sends more touches after those, two seconds apart, such as `'Tap 120 120' 'Swipe "left"'`.

The face chooser added 200 B to the firmware's code image, measured with `cargo objcopy` against its
parent (441,472 B to 441,672 B, the code and its initialised data), and no static RAM. `cargo size`'s
text left out the 2.8 KB of initialised data the image also carries, so it showed 29 B free where the
image left 125 B beside the 49,467 B library. Measure the region with the image itself:
`cargo objcopy --release -- -O binary` of `firmware/` plus the library must fit 491,264 B, the
480 KB region less its header. Its steps run on the same frames as opening an app: with the launcher, a one-button app
calling `choose_watch_face`, and the analog and digital faces installed, opening that app, tapping
into the chooser, swiping left and right and pressing the button peaked at 6,088 B, 3,624 B spare.

Three statics whose first value was all zeros but for a byte or two sat in `.data`, so the image
carried a copy of each: the controller's state before it starts (1,032 B), TrouBLE's default packet
pool (1,036 B) and the app host's last placed glyph (336 B). Each now keeps its empty mark as a zero:
the controller and the glyph hold their value in a `#[repr(u8)]` enum whose empty variant comes
first, and each packet marks itself `in_use` rather than `free`. They moved to `.bss`, taking the
code image from 441,672 B to 439,300 B and `.data` from 2,784 B to 380 B, with static RAM the same,
so the region has 2,497 B free beside the library. The stack did not move: the packages commands
over Bluetooth with a typical install peaked at 7,364 B, 2,348 B spare (7,368 B before), and
opening the launcher's first app at 6,228 B, 3,484 B spare (6,232 B before).

With apps installed and packages commands sent over Bluetooth, as the companion app sends them:

```sh
cargo run -p battletime-packages --example filesystem_use -- --no-theme --image flash.bin launcher=launcher.ntapp ...
python3 -I tools/measure_stack.py firmware/target/thumbv7em-none-eabihf/release/battletime-firmware 3 \
    --flash flash.bin --library shared.ntlib --packages '{"c":"apps"}' '{"c":"install","id":"alarm"}'
```

`--packages` starts the emulator with its Bluetooth bridge and sends each line through
`tools/packages_over_hci.py`, which speaks HCI to the bridge directly, needing neither BlueZ nor
root. The bridge is `pinetime-engine --bluetooth`, whose controller sits on a pseudo-terminal. The
library must be built from the same commit as the firmware, or the firmware faults as it starts and
never advertises. The bridge's controller now and then holds a packet back until more HCI traffic arrives, so
the client nudges it with a harmless command and sends a line again when the watch could not read it.

Static RAM by owner, from `tools/measure_memory.py`. The firmware's own share is mostly Embassy's
task storage, which holds each task's state, and the Bluetooth host's packet pool:

<!-- measure_memory:ram -->
| Owner | Full build | Recovery build |
|---|---:|---:|
| Heap (system 10 KB, UI 20.25 KB; the recovery build has no UI heap) | 30,976 B | 10,240 B |
| Firmware: task storage, the Bluetooth host's resources and packet pool | 21,387 B | 11,474 B |
| Bluetooth controller | 2,618 B | 2,618 B |
| App host (with `battletime-ui`) | 396 B | 0 B |
| Display, graphics and drivers | 372 B | 372 B |
| Other | 5 B | 1 B |
| **Static RAM** (`.data` and `.bss`) | **55,824 B** | **24,736 B** |
| Left for the stack | 9,696 B | 40,784 B |
<!-- /measure_memory:ram -->

The table below is the budget:

| Region | Budget | Holds | Status |
|---|---|---|---|
| UI heap | 20.25 KB | The heap the open app lives in. The `battletime-ui` crate keeps no screen buffer of its own. `battletime-app-host` keeps the open app's state, interpreter and bindings here (1,156 B plus 16 B per expression, the variables' bytes twice and 6 B per variable, at most about 6.6 KB), one 128 B node per widget of the open screen that is not inside a hidden one, each its own allocation so no large free block is needed, plus its text, and the open app's image read from `/apps`. The system's 10 KB shares the same heap. How much an app needs is worked out from its image, as [below](#how-much-heap-an-app-needs). The stopwatch demo's screen holds 1,016 B on the computer and peaks at 1,080 B while drawing. A `TextFile` holds 4 B for every eighth line of its file and the text of the lines near the view, under 2 KB in the small font, and borrows a 512 B buffer while it reads; a 2,000-line note takes about 1 KB of places. A `BitmapFile` holds its picture while its screen is open, at most 1,802 B for 120 by 120 pixels, and frees it before reading another when its number changes. | Partly measured: node size with `size_of` for the watch, the heap peaks in the emulator, the demo screen with a counting allocator on the computer |
| Bluetooth | 12 KB | The controller and the TrouBLE host: connection state, packet buffers and the GATT tables. | Partly measured: the controller is 1.5 KB; TrouBLE's example with it is 12.0 KB in all, executor included |
| Apps | 6 KB | Memory for the app that is open beyond the UI heap, such as the parts of its file read from `/apps`. The runtime's working memory is sized to the app: 1,156 B for the interpreter, 16 B for each expression's bindings, and the variables' bytes for the state and again for the interpreter's scratch room, with 6 B for each variable's place. `AppHost` keeps all three on the UI heap. | Partly measured |
| Stack | 8 KB | The main stack, for interrupts and code that is not in a task. | Planned |
| Tasks | 6 KB | Embassy's tasks. Each task's state is fixed in size when the firmware is built. | Planned |
| Reserve | 3 KB | Left free, so a budget that runs over does not crash the watch. | Planned |
| System state | 4 KB | Settings, notifications, alarms and the step count. The latest 5 notifications are kept on the heap, about 200 B each, beside about 200 B of the shell task's state. | Planned |
| Display buffer | 2 KB | The buffer `battletime-display` fills before each SPI transfer. | Planned |
| Flash store | 2.4 KB | The store's tables, in a static rather than the heap: 1 KB of record keys, 512 B of files, the rings' sector order, the log batches and a 64 B map of the file sectors; an upload's state, about 200 B, is on the heap only while a file is written a part at a time. Its calls borrow up to about 1 KB of stack. | Measured with `size_of` for the watch |

The 64 KB is all the chip has, so a part that needs more must take it from another row of this
table.

### How much heap an app needs

`battletime_runtime::memory_need::estimate_memory_need` works out an app's need from its image alone,
so the package manager can check it before an app is installed or turned on, and the shell before
it opens one. The parts, in bytes:

| Part | Bytes | Where the number comes from |
|---|---:|---|
| The image | its length | Read into the heap while the app is open |
| Working memory | 1,156 | The interpreter without its scratch room, with `size_of` for the watch |
| Sized working memory | `sized_working_bytes` | 16 per expression for its bindings, 6 per variable for its place, and the variables' bytes for the state and again (at most 1,024) for the interpreter's scratch room |
| Subscriptions | 40 each | What the host keeps for each |
| Saved state | `saved_state_bytes` | The buffer the shell writes it from when the app closes, and reads it into when it opens |
| Passing use | 1,536 | Drawing, the image checks at open, and the allocator's gaps |
| Read ahead | 4,096 when the app calls `prefetch_file` | The one file part it holds until a `file_bytes` takes it |
| Folder listing | 1,600 when the app calls `open_folder` | The listed folder's path (64), 64 entries of 8 and 1,024 for their names, reserved when a folder is listed |
| Nodes | 132 per widget of the largest screen, and 40 more for each of a kind that keeps an extra part (`memory_need::holds_extra`: images, icons, buttons, badges, battery and status icons, pickers, app lists, repeats and a few others) | A node and its place in the list; every widget counts as shown, a Repeat counts each row (8 when the count is worked out at run time, only those in view down a list of fixed-height rows), and no screen more than 128 |
| Text | each fixed text's length, 16 for text worked out at run time, 2,560 for a TextFile | |
| Paged code, instead of the image | its resident image, a code cache as large as its longest handler or function and at least 1,024, 160 for the cache's path and list, the widget records of its largest screen with 24 for their window, and 6 for each screen in the screen table; or, if more, the whole image and the resident one together, held while it opens | Chosen only when it makes the total smaller; see [app-runtime.md](app-runtime.md#paged-code) |
| Library icons | each library icon a screen may show at its size, rounded to 4 bytes, plus 16; one a value picks at 1,040; at most 8,192 | The icons the host reads from `/system/icons` for the screen with the most, their sizes from `LIBRARY_ICON_BYTES`, which a compiler test keeps equal to the icons file: an `Image`, `Icon`, `Button` or `Badge` that names one, or takes its image from an expression in an app that can name a library icon at all (a constant from the library's range in its code, a starting value, an image other than its launcher icon that names one, an icon system value or a call to `weather_icon_at`), the notification, Bluetooth and alarm icons for a `StatusIcons`, the charging bolt for a `Battery` and the 8 KB cap for an `AppList`; each icon is at most 1,024 B |

An app's need must fit `APP_HEAP_BUDGET_BYTES`, 27,648 B: the 31,744 B heap less 4 KB for the
system while an app is open. At rest the system holds the shell's index of app ids, about 12 B an
app; the rest of its share is for kept notifications (up to about 1 KB), a small file gathered while it uploads (up to
1 KB), and lines to and from the companion app.

Measured in the PineTime emulator on 2026-10-12 with `tools/measure_heap.py`, which installs the
`digital` face, the launcher and one app, opens each and reads the allocator's bytes in use with
its first screen open, against the estimate (which counts hidden widgets, so it is the larger):

```sh
cargo run -p battletime-packages --example filesystem_use -- --no-theme --image flash.bin \
    digital=digital.ntapp launcher=launcher.ntapp settings=settings.ntapp
python3 -I tools/measure_heap.py firmware/target/thumbv7em-none-eabihf/release/battletime-firmware flash.bin shared.ntlib
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
was 13,188 B of the 31,744 B heap. The shell keeps a full list of every installed app, turned
off or not, only while an app that reads it is open, such as the launcher or the Software app
(`reads_app_list`): 62 B an app measured with 38 installed, against the `LISTED_APP_BYTES` (48 B)
plus id and name such an app's need counts for each. It reads the list once the old app has
closed, and drops it when the next app does not read it, keeping only the face, the launcher, its
look and the listed apps' ids, which every app's need counts as `indexed_app_bytes`. With all 52
apps and faces installed and the `analog` face on, measured in the emulator on 2026-10-14, the face
holds 10,376 B, the launcher 14,780 B and the alarm 11,784 B, against 14,192 B, 14,304 B and
15,600 B when every app kept the full list. With every app in
the repository and the `digital` face installed, the face held 14,356 B and the launcher, listing
every app, 11,832 B; the harness works the launcher's need out at 17,169 B with all 55 apps and
faces on, so it fits with room to spare. A launcher's need is its own, plus the names and icons of
the apps it lists; each icon is read from its image on its own, so no whole image is counted.

The largest app in the repository, `settings`, needed 24.5 KB with the date and time pages, at
128 widgets, the most an app may have, before its face screen gave way to the shell's face
chooser, which took a few widgets off; every app fits with room to spare, and `harness/tests/quality.rs` fails any app or face whose
estimate is over the budget, or whose opening screen builds more nodes than it counts. Before the
saved-state buffer was sized for each app, closing any app borrowed 18,694 B for it, which took
the highest byte touched after leaving `settings` to 30,796 B.

The refusals were checked the same way, over the emulator's Bluetooth bridge, with every app, the
`digital` face and a 36,580 B test image uploaded to `/apps/huge` but not installed:

```sh
python3 -I tools/measure_stack.py <firmware> 5 --launcher \
    --flash flash.bin --library shared.ntlib --packages '{"c":"install","id":"huge"}' \
    '{"c":"enable","id":"launcher","on":false}' '{"c":"enable","id":"digital","on":false}' \
    '{"c":"enable","id":"timer","on":false}' '{"c":"enable","id":"timer"}' '{"c":"apps"}'
```

`huge` was refused with `"why":1,"need":36580`, the launcher and the only face with `"why":3` and
`"why":4`, the timer went off and on again with `"need":16104`, and the launcher still opened with
every app listed, the stack peaking at 7,848 B, 2,632 B spare.

## Measuring again

The firmware, RAM and file-system tables above come from one script, which writes them into
this page between the `<!-- measure_memory:... -->` markers, and the same figures into the
[memory treemap](memory-treemap.html) (`docs/memory-treemap.html`) as its `MEASURED` line. Run it
inside `nix develop`; `--check` only reports whether either page is out of date:

```sh
python3 -I tools/measure_memory.py
```

CI's Memory map job (`.github/workflows/battletime.yml`) runs it on every push and pull request and keeps
both measured pages as an artifact. On master, when the figures change, it commits both pages as
`docs(memory-map): measure <commit>`, so nobody needs to run it before landing; rebase over that
commit as over any other. The prose around the tables still needs updating by hand, and so does the
treemap's hand-written part in `docs/memory-treemap.html`: the flash layouts, the bootloader and
shared library sizes, the deepest stack path and the split of the firmware's static RAM.

It builds the full and the recovery firmware, sorts their symbols by crate, and runs the
`filesystem_use` example of `battletime-packages`. That example also writes the whole 4 MB external
flash with `--image <file>`, for the emulator to load with
`externalFlashMemory LoadBinary @<file> 0`:

```sh
cargo run -p battletime-packages --example filesystem_use -- --image external.bin digital=firmware/apps/face.ntapp launcher=firmware/apps/launcher.ntapp
```

## Watchdog

The bootloader leaves the watchdog running when it starts the firmware, with a 5 second timeout.
It keeps counting while the chip sleeps and pauses while a debugger has it halted. The firmware
must take it over with exactly those settings, because `embassy-nrf` refuses different ones.

The firmware feeds it only while the button is up, so holding the button for about 5 seconds
resets the watch into the bootloader, and holding it 5 seconds more installs the recovery firmware.
If the firmware hangs, the watch resets, and an update that never confirms itself is rolled back.
See [bootloader.md](bootloader.md) for the details.
