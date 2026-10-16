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
  pairing's P-256 maths is built as separate modules, linked at a fixed slot in the code alias,
  stamped with the firmware's build id, and called through a table. They borrow the app's room, so
  they run with the app closed. Apps can call native helpers built with the firmware, such as the
  game rules removed from the runtime, the same way.
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
  fills zeros without an error.
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
   - A busy store: an open file needs the store only when it looks its file up again after a
     write, so the shell's reads now find it busy only during a write. `file_bytes` still reads
     zeros then; telling the app needs an error the runtime can act on.
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
     Reads still wait behind an erase that runs; suspending one (commands 0x75 and 0x7A) remains
     unconfirmed for the XT25F32B.
2. **Load before painting**, with icons still internal.
3. **The resource bundle and safe mode:** icons and accelerometer configurations leave the firmware
   image (about 43 KB), shipped beside it in each DFU package.
4. **The UI arena and the module loader**, proved with a test module that faults on purpose.
5. **Package install and the verifier as a module**, with the install-time function index.
6. **P-256 as a module** when pairing starts.
7. **Paged bytecode functions and glyph subsets**, and the shell's screens as bytecode.
8. **Native code compiled from bytecode**, if an app needs the speed.

## Open questions

- What the split between the UI arena and the system heap costs the app budget, likely 2 to 4 KB.
- Real bus gaps, interrupt overhead and code-alias speed on the watch.
- Whether the XT25F32B suspends an erase (commands 0x75 and 0x7A); unconfirmed, and not modelled
  by the emulator.
- How long one P-256 step takes, and whether the accelerometer configuration can be uploaded in
  chunks.
