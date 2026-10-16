# Architecture assessment

A review, dated 2026-10-12, of whether BattleTime makes the best use of the nRF52832's RAM, internal
flash, external SPI flash and single CPU core, and of five choices: Embassy, the Bluetooth stack,
the UI toolkit, littlefs, and how apps are stored and loaded. Figures come from
[memory-map.md](memory-map.md) and from a release build of `firmware/` at `0068f33`, sorted by
symbol.

Three of its proposals have since been carried out: BattleTime draws its widgets with its own renderer
instead of rlvgl, keeps its files in `battletime-flash-store` instead of littlefs
([flash-store.md](flash-store.md)), and moves the screen's strips and the flash's reads by DMA on a
bus shared one transaction at a time ([The CPU](#the-cpu)).

## In short

| Question | Answer |
|---|---|
| Embassy, or an RTOS? | Keep Embassy. Drawing and flash reads no longer block it; add a second, higher-priority executor for input. |
| Our own Bluetooth host? | No. Keep TrouBLE on our own controller, and trim it. |
| rlvgl, Slint, or our own? | Our own. BattleTime already has its own toolkit; rlvgl only draws six widget shapes. |
| littlefs, or our own storage? | Our own: `battletime-flash-store`, measured faster, gentler on the flash and safe through every power cut tested; the firmware now uses it. |
| Split apps into chunks in ekv? | No: each app is one file read whole when it opens, its saved values one record. Chunk keys cost more lookups than the RAM they save. |

The bigger problem was not these choices but one cooperative executor spinning on the SPI bus.
Drawing and flash reads now go by DMA and let other tasks run, and every flash erase and write
waits on the timer.

## The CPU

The one job that needs exact timing, the Bluetooth link layer, already runs in the radio interrupt
at the highest priority, with PPI starting the radio on time without the CPU. That is what an RTOS
would give it. Blocking elsewhere delays the host's replies but cannot drop the link.

An RTOS would cost RAM we lack. Embassy keeps each task's state in one fixed block sized when the
firmware is built. Eight RTOS threads would need eight stacks, each big enough for its own worst
case, out of the 10.8 KB the stack has now.

What hurt was that the executor often spun instead of waiting. Most of that is fixed:

- **The screen and the flash share the SPI bus by DMA** (`firmware/src/display_dma.rs`). The bus is
  taken one transaction at a time. A strip goes out by chained EasyDMA, in pieces of at most
  255 bytes linked through PPI, while the next strip is drawn into a second buffer; TIMER1's
  interrupt ends each send and wakes whoever awaits `bus_is_free`. Flash reads move by DMA in the
  gaps the display leaves and stop after one piece when the display wants the bus. In the emulator,
  opening the launcher went from 40 ms blocked on the bus to 3 ms.
- **The flash store is async.** Its reads, programs, erases and status polls await the bus and
  wait on a timer, so other tasks run meanwhile.
- **The shell's writes are queued.** The shell saves settings, the theme and app state by queuing
  the file for the storage task, which writes it waiting on the timer; the shell's reads of a
  queued path answer from RAM.
- **The update and recovery slots wait on the timer too.** A firmware arriving by Bluetooth and a
  recovery image moved to a new slot are erased and written through `SlotFlash`, the store's own
  adapter bounded to one slot.

What still spins:

- The shell's store reads run to their end under `block_on`; they only wait on the chip when an
  erase or write from another task is under way, or when a store call other than a single-file read
  meets writes still queued.
- When the store and a slot reach the chip at once, the driver makes the later one spin until the
  earlier one's erase or program ends.

While the CPU spins, touch and the button wait, and the chip cannot sleep, so this costs both
response time and battery.

What to do:

1. **Async SPI with DMA for the screen and the flash.** Done, as above, along with the shell's
   store writes and the slot copies.
2. **A second Embassy executor at a higher interrupt priority** (`InterruptExecutor`) for touch,
   the button and the vibrator. They then preempt drawing and storage. It brings two costs. The
   deepest stack becomes the deepest low-priority path plus the deepest high-priority one, so the
   high-priority tasks must stay shallow. And anything shared across the two priorities needs
   `CriticalSectionRawMutex` or an async lock; the buses' `NoopRawMutex` is not safe there.

RTIC uses the same model as point 2 and would gain nothing over Embassy.

## RAM (64 KB)

Static RAM is 54.7 KB, and the stack has the remaining 10.8 KB. The heap is one 37 KB allocator;
"16 KB storage and 21 KB UI" is only a budget. The largest statics after the heap:

| Static | Bytes |
|---|---:|
| `run_bluetooth` task state | 2,816 |
| `SERVER`, TrouBLE's GATT table | 2,536 |
| `run_shell` task state | 1,568 |
| `HOST_RESOURCES` | 1,496 |
| TrouBLE packet pool, eight 128-byte packets | 1,036 |
| Radio driver state | 968 |
| `BLUETOOTH_PARTS` | 720 |

What to do, in order:

1. **Shrink the storage share of the heap.** Done in part: the flash store keeps its tables in
   static RAM, and storage's share of the heap is now 10 KB. Measure its peak on the watch before
   shrinking it further.
2. **Stop reading whole app images into the heap.** See [Apps](#apps-as-chunks-in-flash).
3. **Move the GATT table into flash** as a constant. Done: `third_party/trouble-host` and its
   macros build the table in a constant, so `SERVER` is 496 B and the values that change take
   366 B beside it.
4. **Fewer packets in the pool**, at some cost to file transfer speed.

## Internal flash (512 KB)

The layout is right. Code must run from internal flash, because the external chip is plain SPI and
cannot run code in place. Fonts belong there too: drawing reads glyph rows constantly, and those
reads would fight the screen for the shared SPI bus.

The build has 362.6 KB of code and constants by symbol, and the full firmware is 368.4 KB of its
400 KB slot. About 50 KB could come back:

| What | Flash | How |
|---|---:|---|
| rlvgl | 23.9 KB | Our own shape drawing; see [The UI](#the-ui) |
| Other constants and padding | 23.3 KB | Not yet looked at |
| Bosch accelerometer configuration | 12 KB | Done: each chip's file lives on external flash |
| Floating-point trigonometry for arcs (`libm`) | 6.3 KB | Done: a table of the sine of each whole degree |

`opt-level = "z"` for crates whose code runs rarely is worth a try. After these, the
firmware/library slot change in [todo.md](todo.md) becomes possible.

Erasing internal flash stops the CPU, the radio interrupt included, for tens of milliseconds. Keep
the shared library's updates to times when Bluetooth is paused.

## External SPI flash (4 MB)

There is plenty of space: the file system's 3 MB holds 172 KB for a typical install. The waste is
in littlefs's 4 KB blocks. Each app's folder costs 8 KB before its image, and a day's step log
takes its own block, about 1.4 MB a year. The firmware update slot (404 KB), the recovery slot
(400 KB) and the 220 KB reserve are fine as they are.

## Bluetooth

Keep TrouBLE on our own controller. The controller is the half where our own code paid off: about
27 KB of flash and 5 KB of RAM less than Nordic's SoftDevice Controller, and no closed binary.

The TrouBLE host is 44.3 KB. A host written for one peripheral connection might save 10 to 20 KB,
but we would own pairing, MTU exchange, ATT's edge cases and every phone's quirks. Firmware
updates arrive over Bluetooth, so a bug there leaves only the bootloader's button hold. Trim
TrouBLE (point 3 under RAM) and come back to this only if flash is still short after the cheaper
gains.

## The UI

BattleTime already has its own toolkit. `app-host` (77 KB) owns the node tree, layout, events, dirty
regions, strip painting, hardware scrolling and transitions. rlvgl is used only to draw six shapes:
button, arc, bar, LED, switch and slider. Each is built when drawn and dropped afterwards.

Replace those six with our own drawing into the `Strip`:

- 23.9 KB of flash back, less whatever our version costs.
- rlvgl's bugs go with it, such as lines that are never drawn.
- Letter edges can mix with what is really underneath, since the strip holds it, which fixes the
  dark fringes on text over coloured buttons.

Slint does not fit. It compiles screens into the firmware when it is built, which clashes with apps
loaded as bytecode and with the rule that no app is built in. Its interpreter needs `std`, and its
runtime is larger than the flash we have left. C LVGL breaks the pure-Rust rule.

## Apps as chunks in flash

### The question

An app is a set of functions and screen layouts that the engine runs. The idea: store each
function, state variable, string and constant as its own entry in [ekv](https://github.com/embassy-rs/ekv),
an async key-value store, and keep everything there, apps included, so each piece loads quickly
without blocking.

### The image is already chunked

The image format ([app-runtime.md](app-runtime.md#the-app-image)) starts with a section table, and
every section that holds many items starts with its own table of (offset, length) pairs. Any one
function, handler, expression, string or widget record is therefore two table look-ups and one read
away. The runtime was designed to read sections in place; only the shell's current habit of reading
the whole image into the heap undoes that.

So splitting apps into chunks is right, and the format already does it. The open question is where
the bytes sit and how the engine reaches them.

### Separate key-value entries cost more than they save

With each chunk as its own ekv entry:

- **Every chunk costs an index search.** ekv finds a key in O(log n) through its LSM tree: several
  small flash reads, each checked, across its levels. A chunk at a known offset in an open file
  takes one read.
- **Code would be moved again and again.** An LSM tree compacts by copying live values into new
  files. App code and constants never change between installs, yet compaction would keep copying
  them, which costs time, erases and wear.
- **Each small write erases a whole page.** ekv's README says each write transaction opens a new
  file and erases at least one 4 KB page, so it asks for writes to be batched. Saving one changed
  variable at a time would erase a sector each time.
- **ekv's format is not stable yet.** A major release may need the store reformatted. Apps can be
  sent again from the companion app, but saved state and settings would be lost.
- **It is built for large data sets.** Its README suggests it from about 1,000 keys, and points to
  linear-search stores such as `sequential-storage` for fewer. A typical install has a few hundred
  chunks at most.

### No on-flash format of our own

A store we write ourselves would have to survive a power cut at every point, track wear, and stay
readable across firmware versions, and every one of those is a place to lose data. A raw app area,
with images laid out by our own allocator, is smaller than a file system but still a format of our
own: moving images to close gaps without losing one in a power cut is the hard part of any file
system, and we would be writing it again.

So every byte on the external flash stays in a format from a tested crate: embassy-boot for the
firmware slots, littlefs for files and `sequential-storage` for small values. What we design is how
the engine reads them, not how they sit on flash.

### Proposed design: store by how the data changes

**1. App images stay in littlefs, read a chunk at a time.** An image never changes between
installs, so it is written once, when it is installed, and only read after that. Reads are what
littlefs does cheaply:

- Opening an app opens its file and reads the header and section table (20 bytes plus 12 per
  section) into RAM. The file stays open while the app is open, which costs 408 B of heap.
- Running a handler or function, or showing a string, seeks to its offset and reads only that
  chunk. Every image in the repository is under 4 KB, so it sits in one block and a seek costs no
  extra flash reads; a larger image walks littlefs's skip list, a few reads more.
- At 8 MHz the bus moves about 1 byte a microsecond, so a 200-byte handler takes about 0.2 ms. That
  read blocks, but for too short a time to matter.
- Hot chunks, such as the expressions run on every redraw and the strings on screen, stay in RAM or
  in the small LRU resource cache planned in [todo.md](todo.md).

The engine never sees littlefs directly: a small `ImageReader` trait with `read_at(offset, buffer)`
hides it, so the store under the images can change later without touching the runtime.

Each image as a flat file, `/apps/<id>.ntapp`, rather than a folder, saves the 8 KB folder each app
now costs, once its state and its `disabled` marker move to the key-value store below.

**2. Data that changes often goes in `sequential-storage`,** a tested pure-Rust key-value store
built for NOR flash, in a partition of its own beside littlefs. It survives power cuts, spreads its
wear, and is async, so its writes wait without blocking. It appends each change to the current page
instead of erasing a page per write.

| Key | Holds |
|---|---|
| `state/<id>` | The app's saved variables, as one value |
| `disabled/<id>` | Present when the app is hidden from the launcher |
| `settings/system`, `settings/theme` | Watch-wide settings |
| `steps/<date>` | A day's step log |
| `notifications/<n>` | Kept notifications |

Keep an app's saved state as one value, written when it changes, rather than one key per variable.
State is at most 2 KB, and one write costs less than many. Every value fits the store's limit of
one 4 KB page.

ekv would fit instead if the key count passes about 1,000 or several keys must change in one atomic
transaction, with its writes batched. Its format is not yet stable, so it is the second choice.

The partition comes out of the 3 MB file system region, for example its last 256 KB. That is a
flash layout change, so it goes in `boot/src/lib.rs` and [bootloader.md](bootloader.md) together,
and the watch's existing file system must be copied across or reformatted when it lands.

**3. What still blocks:** littlefs writes, which now happen only when an app is installed or a file
arrives over BLEFS. Installs are rare and the wearer is waiting for them anyway, and with touch and
the button on the higher-priority executor the watch still responds while one runs.

### What changes

| Step | Now | Proposed |
|---|---|---|
| Open an app | Open its file, read the whole image into the heap | Open its file, read the header and section table |
| Run a handler | Already in RAM, at a heap cost of the whole image | Read only that handler from the open file |
| Save state | Rewrite a file, which may erase a block while the CPU spins | Append one value to `sequential-storage`, waiting asynchronously |
| RAM held while open | The image, plus 408 B for the open file | The section table, the hot chunks and 408 B for the open file |
| Flash per app | 8 KB folder plus the image's blocks | The image's blocks only |

### Measured: the storage benchmark

`tools/storage_benchmark.sh` compiled all 55 apps and faces and ran `tools/storage-benchmark` on
them; both went with littlefs and are in the history up to 9a54d693. Each run formats a simulated flash that counts every read, write
and erase, installs every app, writes a year of step logs, runs 120 days of use (a step slot every
6 minutes, 30 saved states and 20 notifications a day), then opens every app and reads each of its
chunks. Split into keys as proposed, the apps are 1,608 keys: 183 functions of 62 B on average,
189 handlers of 60 B, 942 strings of 7 B and 55 pictures.

The times are flash and bus time only, from assumed chip timings (10 µs a command, 1 µs a byte,
0.6 ms a program page, 50 ms a sector erase), not the datasheet's. CPU time, stack use and code
size are not measured.

| Store and layout | RAM | One chunk read: median, p99 | Open an app: median | One write: median, slowest |
|---|---:|---|---|---|
| littlefs, chunk read from the image file | 1,352 B | 6.1 ms, 10.6 ms | 33 ms | 404 ms, 2.2 s |
| ekv, 1,608 keys, 256 B reads | 1,672 B | 12.6 ms, 15.7 ms | 65 ms | 133 ms, 14 s |
| ekv, 1,608 keys, 256 B reads, writes batched hourly | 1,672 B | 26.7 ms, 34.3 ms | 109 ms | 140 ms, 15 s |
| ekv, 1,608 keys, 4 KB reads | 9,320 B | 49 ms, 49 ms | 255 ms | 133 ms, 13 s |
| sequential-storage, 1,608 keys, page caches and 64 keys | 9,484 B | 3.2 s, 3.3 s | 16 s | 2 ms, 43 min |
| sequential-storage, 1,608 keys, every key cached | 26,236 B | 0.05 ms, 0.3 ms | 0.9 ms | 2 ms, 4.9 s |
| ekv, whole images in 3 KB parts | 1,672 B | none | 45 ms | 133 ms, 12 s |
| sequential-storage, whole images, every key cached | 15,396 B | none | 1.5 ms | 2 ms, 3.9 s |
| ekv, grouped: each app one value, its state one value, settings and notifications one value each | 1,672 B | none | 23 ms | 133 ms, 12 s |
| ekv, grouped, writes batched hourly | 1,672 B | none | 35 ms | 141 ms, 12 s |

What it shows:

- **A chunk in its own key is slower to reach than a chunk in a file.** An ekv lookup searches
  every file of its LSM tree, about 120 flash reads for a 60-byte function, so it takes 12 to 27 ms.
  littlefs takes 6 ms even though the benchmark reopens the file for every chunk, which is most of
  those 63 reads; with the file kept open, a chunk is a read or two.
- **sequential-storage needs every key's place in RAM.** Cleaning a page looks up each item on it,
  and without a cached place each lookup scans the whole store: one write took 43 minutes of flash
  time, and reads took seconds. With every key cached it is the fastest store, at 26 KB of RAM for
  the split apps, more than an open app's whole budget.
- **ekv's writes are slow but rare when batched.** Each transaction erases at least one page:
  686 erases a day writing alone, 57 batched hourly. Its compactions take up to 15 s of flash time;
  they run asynchronously, but a read waits behind them.
- **Grouping halves the time to open an app.** With each app's index inside its image, an app
  opens with two lookups (image and state) instead of three or four: 23 ms on median and 45 ms at
  worst, against 45 ms and 78 ms for the index and 3 KB parts, and 33 ms for littlefs today. The
  writes do not change, since the step log's 240 updates a day dominate them. Rewriting the whole
  ring of five notifications for each new one, 1 KB instead of 200 B, made no measurable
  difference.
- **Batched writes make lookups slower.** Open times were about 50 % higher batched hourly than
  writing alone in every layout, likely because the store holds more uncompacted files between
  batches.
- **littlefs writes block for 404 ms on median,** because each step-log update rewrites the day's
  file. That is the case for the key-value store, and for appending to the step log rather than
  rewriting it.

With only the data that changes in a 512 KB store (saved state, settings, steps and
notifications):

| Store | RAM | Reading an app's saved state: median | One write: median, slowest |
|---|---:|---|---|
| ekv, writes batched hourly | 1,672 B | 3.6 ms | 140 ms, 9.5 s |
| sequential-storage, every key cached | 6,124 B | under 0.1 ms | 2 ms, 3.8 s |
| sequential-storage, page caches and 64 keys | 2,236 B | 85 ms | 2 ms, 3.5 min |

Code size on the watch, measured by `tools/storage-benchmark/size-probe` (in the history up to 9a54d693): a watch build with the
firmware's release settings that calls each store as BattleTime would (mount or format, read, write,
delete, and for ekv a range read to list apps), less the same build with no store:

| Store | Code and constants |
|---|---:|
| ekv, 256-byte reads, checksums on | 32.8 KB |
| littlefs, through `battletime-storage` | 23.7 KB |
| sequential-storage, map with page and key caches | 12.8 KB |

littlefs measures 21.6 KB inside the firmware, so the probe agrees within about 2 KB. Replacing
littlefs with ekv costs the firmware about 9 KB more, out of the 31.6 KB it has free; replacing it
with sequential-storage saves about 11 KB. Two ekv databases on one flash type share one copy of
the code. ekv also brings embassy-sync 0.6 beside the firmware's 0.8, which this count includes.

So splitting apps into keys does not pay: it saves 1 to 3 KB of an open app's RAM, and costs
either 12 ms or more for every function or handler fetched (ekv), or more RAM than it saves
(sequential-storage). Keep each image whole in one file, read in place by offset, and keep the key-value
store for the data that changes. ekv suits that role on RAM, and its writes run asynchronously;
batch them and append the step log.

### Measured: a store of our own

`battletime-flash-store` (`services/flash_store`, [flash-store.md](flash-store.md)) is a store that
knows how data is written, not what it is, so a new kind of data needs no new format or firmware.
It began as a prototype in the benchmark and is now a `no_std` workspace crate with no heap,
generic over the async flash traits the firmware's driver implements; the firmware keeps its files
in it. It has three containers in
the 3 MB region: 2 MB for files, 256 KB for records and 768 KB for logs.

- **Files** are named by path, up to 64 bytes, and written whole: each is a list of up to 64
  sectors in any order, so up to 256 KB, and nothing ever fragments. A write puts the data into
  free sectors, then a header with the path, the sector list and checksums, last; a power cut
  before the header leaves only unused sectors. A newer version retires the older by writing a
  flag into its header, with no erase. Files of 1 KB or less are kept as records with their path,
  so they cost no sector. Paths are found through a table in RAM of 16 bits of each path's hash and
  its first sector, the full path in the header settling any clash; listing reads the headers.
  `TextFile`, `file_bytes` and BLEFS paths keep working as they are.
- **Records** are values up to about 4 KB rewritten often, such as each app's one group of saved
  values and the settings. Every record key carries its owner's app number. Records are appended
  to one shared ring, each with a sequence number and a checksum after its data, and a table in RAM
  points at each key's newest copy. When the ring wraps, the newest copies in its oldest sector
  are carried forward, keeping their sequence numbers, so wear spreads over the whole ring. Two
  sectors are always kept free, so cleanup can finish even after a power cut spoils one.
- **Logs** are appended over time, one entry per day or stream, and drop their oldest sector when
  the ring wraps. A write appends only what an entry gained. Steps, heart rate and activity
  belong here, batched in RAM so each record carries many samples; raw accelerometer recordings
  need a ring of their own, since a day of them would be about 13 MB.

Files and records share one sequence, so while a file moves between the two forms the newer copy
is the one read. Every sector header carries a format version. No operation borrows more than
256 bytes to move data; records are written through a buffer that writes each 256-byte flash page
once.

`--power-cut-test` runs 700 scripted operations on a small flash: app installs up to 14 KB, writes
to six named files that move between small and large (up to 40 KB), deletes, record writes up to
1.2 KB and log appends. It cuts the power at each of the run's 8,413 writes and erases in turn,
leaving that write or erase half done, mounts again and checks every value, every file and the
listing: each must be as it was before the interrupted operation or as that operation left it,
and an older log day may only have lost its start. Each cut is then repeated with a second cut
during the recovery and the operations after it. All 16,826 runs pass. On the way the test found
two bugs, now fixed (a day read from a log included a record a power cut had cut short, and a copy
carried forward was given a new sequence number, which could make an old small file win over a
newer large one), and showed that a ring stalls when its newest copies fill more than about a
third of it; the watch's would fill about a tenth. The crate therefore refuses a record that would
take the ring past half its usable space, limits each app to 4 KB of records, cleans the sector that
gives most space back for its age rather than always the oldest, erases a cleaned sector at once so
a deleted key can never come back, and returns an error, never a panic, when a ring, table or the
file area is full. Its own tests (`cargo test -p battletime-flash-store`) include a smaller version of
the power-cut test with streamed uploads, and the benchmark's larger one, 16,796 runs, passes on the
crate too.

In the benchmark's grouped layout, after 120 days of use:

| Store | RAM | Open an app: median, slowest | One write: median, slowest | Erases a day | Code |
|---|---:|---|---|---:|---:|
| `battletime-flash-store` | 2,700 B | 1.5 ms, 5.8 ms | 0.6 ms, 105 ms | 8 | 26.2 KB |
| ekv, branching factor 2 | 1,672 B | 23 ms, 45 ms | 133 ms, 12 s | 688 | 32.8 KB |
| ekv, branching factor 3 | 1,968 B | 19 ms, 40 ms | 72 ms, 11 s | 497 | |
| ekv, branching factor 4 | 2,184 B | 22 ms, 37 ms | 70 ms, 11 s | 515 | |
| littlefs, a chunk from the image file | 1,352 B | 33 ms | 404 ms, 2.2 s, blocking | 405 | 23.7 KB |

Its RAM is fixed when the firmware is built, with no heap: for 128 files, 128 record keys and rings
of up to 192 sectors, 2,700 B on the watch, mostly the key table (8 B a key), the file table (4 B a
file), the rings' sector order, the log batches and an upload's state. Its code, measured with the
size probe, is about 26 KB, against 23.7 KB for littlefs, which it would replace; a general sort
(7.4 KB) and a second build of the ring code were removed to get there. Its busiest sector,
in the records ring, was erased 14 times in the 120 days, about 40 times a year. Mounting reads
every ring sector to check its records, about 1 MB, roughly a second at start-up; the firmware
could keep a checkpoint instead. ekv's branching factor helps its writes but not its reads, and no
setting removes the cleanup that blocks for 11 s.

## Suggested order

1. Async SPI for the screen and the flash. Done, except that the shell's flash writes and the slot
   moves still spin.
2. A higher-priority executor for touch, the button and the vibrator.
3. Move the firmware onto `battletime-flash-store`. Done; the storage share of the heap is now
   10 KB, still to be measured on the watch.
4. Replace rlvgl with our own shape drawing. Done.
5. The flash trims: the accelerometer configuration, the arc trigonometry, the unexplained 23 KB.
