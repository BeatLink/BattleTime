# External flash as code and resource store

A design study, dated 2026-10-15, of running BattleTime with internal flash holding only the drivers
and the VM, and everything else (apps, the shell's screens, icons, fonts, rare native code) loaded
from the external flash when it is needed and dropped afterwards. It rests on a review of the code,
not of the other docs; where they disagree, [Corrections](#corrections) says so. The numbers under
[Measured](#measured) come from the bench build in `firmware/src/bench.rs`, in the emulator and on
the watch.

## The limits

- **The external flash is a 4 MB XT25F32B** (`drivers/external_flash/src/lib.rs`). The driver
  refuses any other size; slots take the first 1 MB and the file system the other 3 MB.
- **It cannot run code in place.** The nRF52832 has no QSPI, so the chip is reached only over
  SPIM0 at 8 MHz, one wire, with the 0x03 read: about 1 µs a byte, 1 MB/s at best. Fast, dual and
  quad reads would buy nothing on this bus.
- **The bus is shared with the display.** `firmware/src/display_dma.rs` sends 4-row strips
  (1,920 B, 1.92 ms each, 60 to a screen) by chained EasyDMA and slips flash reads into the gaps,
  in pieces of up to 255 B.
- **RAM can run code.** The MPU has one region, the stack guard, with the default map on
  (`firmware/src/stack_guard.rs`), so code copied to RAM runs, best through the 0x0080_0000 code
  alias. Nothing does so today, and nothing is built position-independent.
- **RAM is the real limit, not flash.** The heap is 30,976 B, the app budget 26,880 B
  (`runtime/src/memory_need.rs`), the largest app needs about 24.5 KB, and the stack has 2.3 to
  3.9 KB spare. Less than 4 KB is unused. Loaded code and resources have to share the app's room.

## The model

Think of the watch as a small computer with a memory hierarchy: the interpreter is the CPU,
bytecode its instruction set, RAM a cache and the external flash main memory.

- **Only what never changes is cached.** Code, fonts, icons and strings are read-only, so dropping
  them costs nothing. App state, the Bluetooth link and the screen stay in RAM and are saved only on
  events, as now. Swapping RAM out to flash would cost an erase (about 45 ms) and wear each time.
- **Bytecode pages; native code does not.** Without an MMU or position-independent code, native
  Rust can only be loaded as whole modules at a fixed place. Bytecode can be loaded a function at
  a time without the app knowing: `Frame` keeps only the running function's number and an offset,
  and looks the caller's code up again on return (`runtime/src/interpreter.rs`).
- **Some things can never wait for a load:** the radio interrupt (about 100 µs deadlines), whatever
  serves the load itself, painting, and anything needed while the external flash is broken.

So internal flash keeps a resident core, about 250 KB:

- Bluetooth: our controller and TrouBLE, DFU and BLEFS.
- The flash store and driver, display DMA and the other drivers, core, alloc and embassy.
- The interpreter, and the renderer and layout, which run on every strip.
- The loader, and the safe-mode screens with the 2.4 KB bitmap font.

Everything else moves to the external flash: apps, the shell's screens rewritten as bytecode, the
shared library's icons and accelerometer configurations, every font, and rare native code. That
would leave about 230 KB of internal flash free, against 13 KB today.

## How the pieces load

- **Load before painting, per frame.** Layout already knows which nodes a frame shows, so before
  each paint the host loads what that frame draws: icons, image run indexes, strings, the code of
  bound expressions, and the font glyphs the laid-out text uses. Between frames anything may load,
  so a screen has no size limit; a list streams rows in and out as it scrolls.
- **Fonts are glyph subsets.** Every font travels the way the dot-matrix font does since
  a84c7877: inside the image that uses it, installed when the image opens. Only the characters
  layout has met are kept in RAM, about 0.5 to 1.5 KB, and they stay across screens.
- **Functions are paged.** The installer writes beside each app an index of its functions, the
  callee set of every handler and expression, and a checksum per page; the host keeps a 2 to 3 KB
  cache of functions and runs a handler only when its whole set fits.
- **Native modules for rare work.** Code such as package install, the verifier, settings import and
  pairing's P-256 maths is built as separate modules, linked at 0 with their relocations kept, and
  placed wherever the heap has room: the loader moves each address in them by where they landed,
  through the code alias for code. They call the firmware only through a table whose ABI number
  both sides check, so a module outlives firmware builds until that table changes. Apps can call
  native helpers built with the firmware, such as the game rules removed from the runtime, the
  same way.
- **Later, native code for apps compiled from their bytecode.** At install the compiler could turn
  verified bytecode into Thumb code with bounds checks and step counters kept, for an estimated
  3 to 10 times the interpreter's speed.
- **Safe mode.** When the resource bundle is missing or fails its checksum, the watch runs its
  status page, Bluetooth, DFU, BLEFS and the clock, and no apps, until the companion sends the
  bundle again. A dead external chip already stops Bluetooth today (`firmware/src/main.rs` returns
  before starting it), so this design adds no new way to lose the watch.

## Measured

The bench firmware (`cargo build --release --features bench`) runs in place of the shell and
`tools/bench_external_flash.py` prints its results, from the emulator or from a dump of the
diagnostics characteristic of a watch running it. Reads and redraws are timed with TIMER2, which
keeps counting while the CPU sleeps waiting for the bus; the loops with the DWT cycle counter.

The watch ran it on 2026-10-15, built from b981a989. The emulator charges the bus 1 µs a byte with
no gaps between pieces and fetches from RAM for free; the watch's reads came out 5 to 30% slower,
and its code from RAM differs, as below.

**Reads with the bus idle**

| Read | Raw, emulator | Raw, watch | Store, emulator | Store, watch |
|---|---:|---:|---:|---:|
| 32 B | 58 µs | 74 µs | 178 µs | 187 µs |
| 256 B | 286 µs | 305 µs | 384 µs | 394 µs |
| 1 KB | 1.06 ms | 1.12 ms | 1.19 ms | 1.25 ms |
| 4 KB | 4.18 ms | 4.42 ms | 4.30 ms | 4.53 ms |

A 600 B file kept as a record took 0.98 ms in the emulator; the watch had no such file, so it was
not measured there.

On the watch a read costs about 1.08 µs a byte plus 40 µs. The store adds about 120 µs to every read, from finding
the path again and running the call on the heap under `block_on`, so it triples the cost of a small
read; open file handles would remove most of it.

**Full-screen redraws**

| Redraw | Emulator | Watch | Change on the watch |
|---|---:|---:|---:|
| Nothing to draw | 120 ms | 123 ms | |
| 0.5 ms of drawing per strip | 120 ms | 124 ms | 0% |
| … and a 64 B read on every strip | 125 ms | 129 ms | +4% |
| … and a 256 B read on 8 strips | 123 ms | 126 ms | +2% |
| … and a 256 B read on every strip | 137 ms | 141 ms | +14% |
| … and a 1,920 B read on every strip | 238 ms | 248 ms | +100% |
| … and a 4 KB read in the background | 124 ms | 128 ms | +3% |

The bus, not the CPU, sets the pace: drawing a strip overlaps sending the one before, so it costs
nothing, while every byte read costs the frame about the same microsecond whether it is read before
painting or during it. Loading a frame's resources before painting therefore matters for keeping
frames even, not for their total time. Only reading as much as each strip sends doubles a frame.

**Code run from flash and from RAM**

| Loop, on the watch | Internal flash | RAM at 0x2000_0000 | RAM's code alias, 0x0080_0000 |
|---|---:|---:|---:|
| 20 B, which the flash cache holds | 34.1 ms | 37.7 ms | 28.3 ms |
| 3 KB, more than the cache holds | 4.04 ms | 4.96 ms | 3.22 ms |

On the watch, code in RAM run through the code alias was the fastest of the three: 17% faster than
internal flash for a loop the flash cache holds and 20% for one it does not. The same code run at
its 0x2000_0000 address was the slowest, since its instructions then share the system bus with
data. The emulator showed no difference for the small loop and missed the system bus's cost.

The same watch, running master, missed the flash cache on 10.3% of instruction fetches (32.6
million of 317 million) over that time, so most firmware code runs at the slower, missing speed
some of the time.

**What this means for the design**

- Opening a screen that loads 3 to 10 KB costs 3 to 10 ms on top of a 120 ms redraw.
- A frame that streams a few hundred bytes costs a few per cent, too little to see.
- Only full-colour pictures that stream every row run at half speed.
- Native modules loaded into RAM run faster than code in internal flash, if they are linked to
  run from the code alias.

## Corrections

Docs and comments the code contradicts, as of e30358e4:

- `StoreFlash::read` (`firmware/src/storage/watch.rs`) is an `async fn` that calls the blocking
  `read_flash`, which sleeps the whole executor in WFE. Its comment, `docs/firmware.md` and
  `docs/storage.md` said store reads await the bus; they are corrected on this branch, and
  [architecture-assessment.md](architecture-assessment.md) still says so.
- `display_dma.rs`'s header and `docs/storage.md` said a flash read never slows drawing by more
  than one piece. A read may resume before each of `set_window`'s three commands and before the
  pixels, up to four pieces a strip, every 64 B in blocking fills, and in full on the async
  `ready()` path, which never sets `DISPLAY_IS_WAITING`. Both are corrected on this branch.
- The shell's store calls use `try_lock`; a busy store reads as nothing, and `file_bytes` then
  fills zeros without an error. The shell's waits keep this from happening today: the watch
  counted no busy store call (see [Order of work](#order-of-work)).
- "About 12 KB of RAM per app" (CLAUDE.md, `docs/app-runtime.md`) appears in code only as a test
  constant (`harness/tests/quality.rs`); the firmware's limit is 26,880 B.
- [architecture-assessment.md](architecture-assessment.md)'s launcher figure, 40 ms on the bus
  down to 3 ms, has no measurement behind it in code or tests.

## Order of work

0. **Measure**, emulator first, then this bench on the watch. Done.
1. **Store and bus fixes:**
   - Open files: done. `Files::open_now` keeps a file's map, so a read is one DMA read with no
     look-up or store lock, 62 µs for 32 B against 178 µs, for 2 KB of flash. The shell's
     `read_file` reads through the file it last opened.
   - Awaiting store reads: measured at 4.6 KB of flash, since every store function that reads
     grows by an await, and left out. Only background tasks would gain; the shell's reads run to
     their end either way.
   - A busy store: the shell awaits `wait_until_free` after every wake, before each drawing step
     and before closed apps run, and handlers and layout then run without yielding, so no other
     task can take the store from under them. The watch confirmed it: on 2026-10-15,
     `StoreCallsFoundBusy` read 0 after 3,150 shell loops that included three app installs over
     Bluetooth. So no retry was built; paged code that reads from inside an await would need one.
   - Pausing reads for the painter: tried and left out. A 4 KB read started in the background
     streams to its end while the painter awaits the bus, so a strip waited up to 3.6 ms instead
     of 1.4 ms; asking the read to pause from the painter's await cut that to 1.4 ms with no change
     to the frame's total. But the flag it raises can stay up while the painter is suspended, and a
     blocking store read in another task, sleeping in WFE until its read moves, then never starts:
     the watch hangs. Holding the bus across `set_window`'s three commands is safe, being one
     blocking call, but cost 728 B to remove gaps of a piece or two. Both wait for awaited store
     reads, under which a paused read no longer blocks the executor.
   - Sectors erased ahead: done. `prepare_room` keeps the next four free file sectors erased, so
     a file of up to 16 KB, such as an app image, is written without an erase, for 704 B of flash.
     Reads still wait behind an erase that runs. The XT25F32B does suspend one, measured on the
     watch by the bench's `measure_erase_suspend`: a sector erase takes 50.0 ms left alone; asked
     to pause (0x75) about 2 ms in, the chip is idle 94 µs later with status register 2's SUS bit
     set, a read while paused returns the right bytes, and after resuming (0x7A) the erase ends
     48.1 ms later with the sector erased. The emulator does not model it. So a read need wait at
     most about 0.1 ms behind an erase; the store does not use this yet.
2. **Load before painting.** Done for the shared library's icons, the largest part: after layout,
   when a list scrolls and when the launcher's icons arrive, the app host reads the library icons
   the shown widgets draw into the heap, up to 8 KB, and drops the rest, so painting never waits on
   the flash. Layout sizes an icon not yet read at the 32 by 32 all of them share. The image run
   indexes, strings and bound-expression code still to come; they wait for paged bytecode.
3. **The resource bundle:** done without a bundle. The accelerometers' configuration files and the
   icons left the shared library for `/system` on the external flash (43 KB together: the library
   went from 49,467 B to 6,731 B, and the firmware region gained 38,652 B after the 4,084 B of code
   that reads them). Rather than ship them in each DFU package, the companion puts any the watch
   lacks whenever it connects, and after `battletime flash`; a missing file means no steps or empty
   boxes for icons, not a watch that will not start, so the status page, Bluetooth, updates and
   files all keep working without them, which is the safe mode this stage asked for. Each file is
   checked only by its size, so new artwork of the same size would not be put again; a checksum in
   the listing would close that.
4. **The module loader:** done, without a UI arena. A module takes its room from the heap like
   any allocation and gives it back when its call returns; one that does not fit beside the open
   app answers that the heap has no room, so no part of the heap is set aside for modules. The
   `module` packages command ([companion-protocol.md](companion-protocol.md#packages-served)) runs
   a call; `battletime apps module <name> [call]` sends it. `modules/test` proves it: call 0 reaches
   its answer only through the heap, its data and function addresses the loader moved (the compiler
   loads some through MOVW and MOVT pairs, which the loader moves too, not only through whole
   words), call 1 panics and call 2 runs an undefined instruction. A crash inside a module records
   `mod:<name>` as the open app, and the next start counts it against that module's file in
   `/system/modules/crashes`; after two the module is not run again until its file changes.
   `tools/check_modules.py` checks all of this but the restart in the emulator. The loader,
   counting included, costs 7.1 KB of firmware code; the test module is 364 B on the flash, and
   the verifier, built as a trial module, 14.7 KB, about twice its share of the firmware, since it
   carries its own copy of the image parser and instruction decoder. A panic's return addresses
   inside a module are not recorded, since the crash record looks only in the firmware's code.
5. **The verifier as a module:** done. The firmware carries no verifier: `modules/verifier`
   (15,360 B on the flash) checks an image's code once, in the shell, with the open app closed,
   which leaves it room beside even the largest image (Settings, 7.2 KB) within the app budget. An
   install answers only once that ran, behind an Installing page, and an image it passed is named
   by CRC and length in `/system/verified`, for this firmware build, so it opens at once
   afterwards; any other image, a temporary app or every app after an update, is verified before
   its first open behind a Checking page. Closed apps that are not named wait until they open. The
   firmware lost 4.2 KB of verifier code to the stage-4 loader's 7.1 KB, so both together cost
   2.8 KB, and the stack spare across an install went from 2.4 KB to 3.4 KB. No install-time function
   index was needed, as paging found. The rest of the install, the facts and id checks, could join
   the module later, for 2 to 4 KB more, while listing, turning apps on and off and the launcher's
   order stay in the firmware, which the shell needs at every turn.
6. **P-256 as a module:** done ahead of pairing. `modules/p256` (8,548 B on the flash, none in the
   firmware) holds the `p256` crate's ECDH, the one TrouBLE's security feature uses. Its call 0
   works out the DHKey of the Bluetooth specification's P-256 sample data and answers the
   microseconds it took: 240 ms in the emulator, which runs about as fast as the chip; the watch
   gives the real figure. Run from the packages command it leaves 2.3 KB of stack spare. Pairing
   will add calls that take a private key and the phone's public key in and give the DHKey out,
   and a random key pair, once the host table offers the radio's random numbers.
7. **Paged bytecode functions and glyph subsets**, and the shell's screens as bytecode. Paged code
   is done ([app-runtime.md](app-runtime.md#paged-code)), ahead of the stages before it, while
   apps are still small: an app whose need is smaller that way keeps its expressions, handlers and functions on
   the flash and reads them into a code cache of at least 1 KB as they run, and holds only the open
   screen's widget records, for 6,812 B of firmware code. Today it pages 13 of 66 apps and faces,
   saving up to 3 KB each; the gain grows with an app's code and screens. No install-time index was
   needed: the image's own tables give every piece's place, and the bindings and screen table are
   worked out from the whole image while it opens. Closed apps are left whole: they are loaded only
   for a moment, and opening paged holds both images then anyway.

   Strings stay in the resident image: paging them was built and measured, then left out, as
   [Paging strings](#paging-strings) explains. The shell's notifications and quick settings left
   the firmware for `/system/shell`, which saved 3.5 KB; the shell opens and verifies them like any
   app, so with stages 4 to 6 the firmware ends 0.7 KB smaller than before the loader. Glyph
   subsets remain.
8. **Native code compiled from bytecode**, if an app needs the speed.

## Paging strings

Strings are the last part of an app's image a paged app still keeps in RAM: 14.6 KB across the 66
apps and faces, at most 1.3 KB in one (`settings`). Paging them was built and measured on
2026-10-20 and is not on `dev`; the work is commit 1a1495b2 on the branch `worktree-string-paging`.

### How it was built

- **Which strings stay.** A resident image keeps the app's id, name and launcher group, its
  variables' keys and its screens' names, since saving state and finding a screen by name read them
  with no image file at hand. The rest stay in the file: in the resident strings table each such
  entry has the top bit of its length set and its offset into the whole image's strings
  (`paging::strings_kept`, `AppImage::string_place`).
- **The string cache.** A `StringCache`, packed and dropping the least recently used like the code
  cache, reads a string the first time it is needed, with the same CRC check of the image file.
- **The interpreter.** Its text work (joins, slices, lengths, patterns, file paths) copies a paged
  string out of the cache before using it, since the cache may drop it while the code goes on. Once
  a run ends, it reads any paged string its result or its effects' arguments name, so the host and
  the shell find them by number with no file reader at hand.
- **Before layout and painting.** Text arrays, such as a `Roller`'s options or a `Dropdown`'s, hold
  strings by number and are drawn while painting, which never waits on the flash. The host reads
  every string the nodes' text arrays name into the cache before layout, as it reads library icons.
- **Only where it saves.** The cache must hold every string a screen's text arrays could name at
  once, so it is sized to those arrays and twice the longest string, at least 256 B, up to all the
  strings it pages. An app pages its strings only when they take more than that cache and its
  160 B of bookkeeping (`paging::pages_strings`).

It passed every test and every harness snapshot, and `settings` worked in the emulator.

### Why it is not on dev

Measured with `settings`, the app it helps most:

| | Without | With |
|---|---:|---:|
| Firmware code | 448,368 B | 454,688 B (+6,320 B) |
| Peak stack, opening `settings` | 6,340 B | 7,132 B (2,324 B spare) |
| Heap with `settings` open, measured | 14,128 B | 14,236 B (+108 B) |

- **Little to move.** `settings` leaves 642 B of strings in its file, and the cache that reads them
  back must hold a screen's text arrays at once, 288 B for it. With the cache's own bookkeeping only
  3 of the 66 apps and faces would page strings at all, and the one measured held more heap.
- **Stack.** Each paged string is copied into a 256-byte buffer on the stack before use, inside the
  interpreter's deepest frame.
- **Code.** The string cache, its generic reads and the staging pass cost 6.3 KB of internal flash.

### How it could pay later

1. **When an app has a lot of text**, such as a reader, a help app or one translated into several
   languages, with several kilobytes of strings. `pages_strings` already pages only then.
2. **One reader, less code.** Share one image file reader between the code and string caches, and
   read through `dyn SystemValues` rather than one copy of the read for each kind of system.
3. **No stack copies.** Copy a paged string straight into the interpreter's text buffer, or read it
   into the cache in a step before the instruction runs, rather than into a 256-byte stack buffer.
4. **Strings grouped by screen.** If the compiler wrote each screen's strings together, list labels
   especially, a screen's strings would load in one read, as its widget records do, and the cache
   would hold only the open screen's text instead of every text array at once.

With 2 and 3 it should cost a couple of kilobytes of code and no stack, and with 4 its cache
shrinks to one screen's text; it is worth bringing back once an app's strings reach several
kilobytes.

## Open questions

- Real bus gaps, interrupt overhead and code-alias speed on the watch.
- How long one P-256 step takes, and whether the accelerometer configuration can be uploaded in
  chunks.
