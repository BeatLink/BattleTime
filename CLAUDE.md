# NeoTime: notes for Claude

NeoTime is a Rust firmware for the PineTime that merges the best of InfiniTime and wasp-os. It is its
own product, licensed GPL-3.0-or-later, so code ported from InfiniTime or PinetimeLite is fine.
Start with [README.md](README.md) and [docs/architecture.md](docs/architecture.md); code style is in
[docs/coding-guidelines.md](docs/coding-guidelines.md).

## Working alongside other sessions

- Several Claude sessions work on NeoTime at once, each in its own worktree under `.claude/worktrees`.
- Talk to the other sessions whenever it helps: find them with `ListAgents` and message them with
  `SendMessage`. Agree on an order when one feature depends on another's landing, say which files
  you are about to change when they might be editing them too, and ask before touching a part
  another session owns.
- Work lands by pushing to GitHub's `master`, so the local `master` is often behind. Run
  `git fetch origin`, rebase onto `origin/master`, rerun the tests, then push without force.
- Never name a commit to anyone until it is on `origin/master`.
- Never revert or reformat another session's files. Run `cargo fmt -p <crate>`, not a bare
  `cargo fmt` at the root.
- `Cargo.lock` is the usual merge conflict; regenerate it rather than hand-merging.
- Before using the real watch, tell the other sessions, so only one connects at a time.

## Every change

- Tests go in the same commit as the change, and coverage never drops. `tools/check_coverage.sh`
  inside `nix develop` runs both gates (Rust floor in the script, TypeScript floors in
  `vite.config.ts`). When coverage rises, raise the floor in the same change.
- App behaviour is tested in `harness/` ([docs/testing.md](docs/testing.md)), not vitest. A change to
  what an app shows rewrites its snapshots with `NEOTIME_UPDATE_SNAPSHOTS=1` in the same commit.
- Docs ship with the code, never as a follow-up. A new crate gets its place in
  `docs/architecture.md`; a change to RAM or flash use updates `docs/memory-map.md`.
- Open work lives in `docs/todo.md`. When an item is done, remove it in the same change and record
  it in `docs/features.md` if the wearer sees it, or in `docs/architecture.md` or the part's own doc
  if not. Before removing an item, check in the code that it is really done; keep any open part as a
  narrower item.
- Anything to look at (screenshots, galleries, write-ups) goes in `docs/` and lands with the change.
  Reference existing images such as `harness/snapshots/screens/*.png` so they stay current. The
  PineTime screen is square, 240x240; never draw it round.
- New drivers go in `drivers/<name>` and get a simulated part in `simulator/` plus a line in
  `docs/simulator.md`.

## Design rules

- Going back retraces navigation exactly: it revisits every screen passed on the way forward, in
  reverse order and moving the opposite way (face, notifications, quick settings goes back through
  quick settings, notifications, face). The shell records this in `neotime_app_host::way_back`; a new
  way to move forward must record its step, and a new way back must take it.
- The firmware carries only the engine, plus the notifications and quick settings screens from
  `shell/`, which it compiles in. No other built-in apps, faces, launcher, icons or fonts, and no
  fallback copies of them. Apps live in `/apps` on external flash; the shared library sits in the
  last 92 KB of internal flash (0x69000).
- Anything that changes often (notifications, settings, app state) goes on external flash, never
  internal flash.
- Apps are Preact and Signals compiled on the computer to bytecode ([docs/app-runtime.md](docs/app-runtime.md)).
  About 12 KB of RAM per app rules out any JavaScript engine or Wasm on the watch.
- New settings touch `services/settings`, `app-host/src/settings.rs`, `services/companion`, the
  runtime's system values, the compiler and the sdk. The theme stays in `/settings/theme`.
- Pure Rust only; no closed-source blobs and no C where a bug could brick the watch.
- Flash layout changes go in `boot/src/lib.rs` and `docs/bootloader.md` together.

## Memory budget

- Keep at least 2 KB of stack spare, measured with `tools/measure_stack.py`. Large state goes in a
  `Box` or lives on the stack only while it is used; a big stack frame overflows the watch's stack,
  which shows as a blank screen and then a rollback.
- The full firmware nearly fills its 388 KB slot, so a new firmware feature must find room first.
- CI measures `docs/memory-map.md` with `tools/measure_memory.py`.

## The watch cannot be recovered by wire

There is no SWD probe, so a watch that cannot reach the bootloader is lost. Recovery rests on the
bootloader at 0x0 (rollback of an unconfirmed firmware, a long button hold for the recovery
firmware) and on DFU in the firmware.

- Never write the bootloader pages from firmware, and keep the recovery slot valid.
- Feed the watchdog only from the main loop and only while the side button is up.
- Prove bootloader, reloader and update changes in the emulator suites under `tests/emulator` before
  any watch, and only flash with the user's go-ahead.
- After a DFU update something must confirm the firmware (writing the time, or any GATT read), or
  the next reset rolls it back.

## Traps

- **Reading address 0 is undefined behaviour.** The bootloader sits at the null pointer. Reading it
  through a Rust pointer or a HAL slice lets LLVM silently delete code after it (a 305 KB firmware
  once built at 44 KB). Use asm loads, as `status::BootloaderPages` does, and compare `cargo size`
  with master after touching such code.
- **Dropping an embassy `Output` floats its pin.** Keep every display pin's `Output` alive while the
  screen is in use, or the reset pin drops and the panel goes blank.
- **The emulator runs delay loops at the chip's speed**, about 50 million instructions a second, so
  MCUBoot's start-up takes about 20 emulated seconds. The Renode emulator it replaced ran them
  about twice as fast; a test tuned there may need a longer wait.
- **The packages example writes an all-black theme.** `filesystem_use` stores `/settings/theme` as
  zero bytes, so the launcher and apps render black in the emulator; build the image without it.
- **`tools/measure_stack.py` starts with erased external flash**, so it measures only the help
  screen, not a real install.
- **littlefs-rust is patched.** `third_party/littlefs-rust` raises a loop limit that panics on blocks
  written by C littlefs, and the workspace builds `littlefs-rust-core` without overflow checks.
- **Bootloader builds are not byte-reproducible across checkout paths**; the code is the same.
- **`cargo build -p <dev-dependency>` panics in cargo**; check such a dependency from a throwaway crate.
- **Never `pkill -f` a pattern that also appears in your own command line**; it kills the shell.
- **The InfiniTime checkout beside this repo is a local reference copy.** Never push it.
