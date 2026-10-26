# BattleTime repository

GPL-3.0-or-later, two Cargo workspaces: [battletime/](battletime/) (the watch) and [companion/](companion/) (phone/desktop app). Run cargo and npm from inside the half you work on. Paths below are from the repo root. Start with [README.md](README.md), [docs/architecture.md](docs/architecture.md), [docs/coding-guidelines.md](docs/coding-guidelines.md).

- A protocol change ([docs/companion-protocol.md](docs/companion-protocol.md)) updates firmware, doc and companion in one commit where it can.

## Working alongside other sessions

- Other sessions work in worktrees under `.claude/worktrees`; find them with `ListAgents`, message with `SendMessage`. Say which files you are about to change, agree an order for dependent features, ask before touching what another session owns.
- Land by pushing to `master`: `git fetch origin`, rebase onto `origin/master` (after a folder move: `git -c merge.directoryRenames=true -c merge.renameLimit=20000 rebase`), rerun tests, push without force.
- After each push run `git -C <main checkout> merge --ff-only origin/master`; keep it on `master`, never detached. If its tree has changes or the merge refuses, tell the user; never stash or discard.
- Never name a commit to anyone until it is on `origin/master`.
- Never revert or reformat another session's files; `cargo fmt -p <crate>`, never a bare `cargo fmt`.
- Regenerate a conflicted `Cargo.lock`, never hand-merge it.
- Tell the other sessions before using the real watch; only one connects at a time.

# The watch: battletime/

## Every change

- Run only the tests for the code you changed, then land it. No whole-workspace suites, `tools/check_coverage.sh`, workspace clippy, snapshot sweeps or firmware builds for parts you did not touch; CI runs those. New tests go in the same commit.
- Docs ship with the code: a new crate goes in `docs/architecture.md`, a RAM or flash change updates `docs/memory-map.md` and `docs/memory-treemap.html`; a new driver gets a simulated part in `battletime/simulator/` and a line in `docs/simulator.md`; flash layout changes touch `battletime/boot/src/lib.rs` and `docs/bootloader.md` together.
- App behaviour is tested in `battletime/harness/` ([docs/testing.md](docs/testing.md)); a change to what an app shows rewrites snapshots (`BATTLETIME_UPDATE_SNAPSHOTS=1`) in the same commit.
- Each app has a tour `battletime/tests/tours/<id>.battletime` and a page `docs/apps/<id>.md`; a new app or new/changed screen updates both in the same commit.
- Anything to look at (screenshots, write-ups) goes in `docs/` and lands with the change. The screen is square, 240x240; never draw it round.
- Open work is `docs/todo.md` (companion: `companion/TODO.md`). Remove a finished item in the same change after checking the code, and record it in `docs/guide/` (wearer-visible) or the part's own doc; keep any open part as a narrower item.

## Design rules

- Going back retraces navigation exactly, in reverse, moving the opposite way. A new way forward must record its step in `battletime_app_host::way_back`; a new way back must take it.
- Side-button click always means back; apps never hear it. Never bring back swipe-right-to-go-back or a swipe that opens Settings.
- App pages run left/right with a wipe and a page bar, never dots; vertical scrolling only for lists inside a page.
- Firmware carries only the engine: no built-in apps, faces, launcher, icons or fonts, and no fallback copies. Shell screens compile from `battletime/shell/` to `/system/shell`, rare native code is a module in `/system/modules`, apps live in `/apps`, and icons, text font and accelerometer config in `/system`, all on external flash (the companion installs `/system`). The shared library ships inside the firmware image.
- Anything that changes often (notifications, settings, app state) goes on external flash, never internal.
- New settings touch `battletime/services/settings`, `battletime/app-host/src/settings.rs`, `battletime/services/companion`, the runtime's system values, the compiler and the sdk. The theme stays in `/settings/theme`.
- Pure Rust only; no closed-source blobs, no C where a bug could brick the watch.

## Memory budget

- Keep at least 2 KB of stack spare (`tools/measure_stack.py`). Big state goes in a `Box`; an overflow shows as a blank screen then a rollback.
- The firmware and shared library share a 480 KB region with about 44 KB free; read-only, rarely used data belongs on external flash ([docs/external-flash-architecture.md](docs/external-flash-architecture.md)).

## The watch cannot be recovered by wire

No SWD probe: a watch that cannot reach the bootloader is lost. Recovery rests on the bootloader at 0x0 and DFU in the firmware.

- Never write the bootloader pages from firmware; keep the recovery slot valid.
- Feed the watchdog only from the main loop and only while the side button is up.
- Prove bootloader, reloader and update changes in `battletime/tests/emulator` before any watch; flash only with the user's go-ahead.
- After a DFU update something must confirm the firmware (write the time, or any GATT read), or the next reset rolls it back.

## Watch traps

- **Reading address 0 is undefined behaviour.** LLVM silently deletes code after a Rust-pointer read of the bootloader; use asm loads like `status::BootloaderPages`, and compare `cargo size` with master.
- **Dropping an embassy `Output` floats its pin.** Keep every display pin's `Output` alive or the panel goes blank.
- **Big futures and statics land on the stack.** Box them; poll a big awaited future through an `#[inline(never)]` wrapper; `*self = Big::new()` and `StaticCell::init(array)` build on the stack, so use `ConstStaticCell`.
- **A blocking wait in WFE must SEV afterwards**, or the executor sleeps with a task ready.
- **A slow draw resets the watch** ("Reset: watchdog"). Widgets drawing many lines must skip those outside `DrawContext::clip`.
- **New bytecode instructions need the analysis taught.** One that writes an array must call `note_write`; `derive_inputs_in` in `battletime/app-host/src/subscriptions.rs` and the bindings must learn every instruction touching state; pass a library function's return type as `expected`, or a `Color` comes back as text.
- **A test compile error can pass unseen.** Grep test output for `^error` as well as `FAILED` and `panicked`.
- **`tools/measure_stack.py` measures whatever ELF sits at the release path**; the recovery build lands in `target/thumbv7em-none-eabihf/recovery/`. `--packages` needs a library built from the same commit as the firmware, or the firmware faults at start.
- **The emulator models no chip anomalies** (e.g. nRF52832 SAADC anomaly 86); code driving peripheral registers itself needs a check on the watch.
- **`cargo build -p <dev-dependency>` panics**; check it from a throwaway crate.

# The companion: companion/

Paths are relative to `companion/`.

- Bluetooth stays behind `src/ble/transport.ts`; screens use `useWatch`, never the bridge directly.
- Every step in `core/src/ble.rs` has a timeout and a failed connect drops the link, or later attempts wait on services that never resolve.
- Verify Rust with `cargo test --workspace` and `cargo clippy --workspace --all-targets` in `nix develop .#companion`; the Tauri crate needs a `dist/` from `npx vite build`.
- Phone features sit behind `phoneBridge` in `src/services/phone`; check `phoneBridge.kind`, `supportsCalls`, `supportsLocation`, never the platform.
- Offer a feature only when the watch serves it: decide with `watchReady` and `capabilities` from `useWatch`, not `connection`.
- `src/protocol/battletime.ts`, `file-service.ts`, `gadgetbridge.ts`, `requests.ts` mirror [docs/companion-protocol.md](docs/companion-protocol.md); change the doc first. The Rust copies (`core/src/protocol/battletime.rs`, `packages.rs`, `app_image.rs`, `core/src/gadgetbridge.rs`) change in the same commit.
- `src/bundles/watch-apps.ts` is generated (`npm run import-watch-apps ../battletime/dist/store`, see [docs/app-store.md](docs/app-store.md)); never hand-edit it; rebuild when widget or instruction numbering changes or an app is added.
- After a firmware update the app must exchange GATT data (it writes the time), or the watch rolls back.
- Companion tests ship in the same commit; run only changed files (`npx jest <path>`). A `package-lock.json` change needs a new `npmDepsHash` in `flake.nix` (`nix run nixpkgs#prefetch-npm-deps -- package-lock.json`).

## Companion traps

- **A BlueZ link can wedge**: `bluetoothctl info` says `Connected: yes` with no RSSI and `disconnect`/`remove` hang. Only switching the adapter off and on clears it, dropping every Bluetooth link on the computer, so ask the user first.
- **Run `npm ci` in `nix develop .#companion` in your own worktree**; never link another worktree's `node_modules`.
- **Preact is ESM only**; a new ESM-only dependency must be added to `transformIgnorePatterns` in `package.json`.
- **Node's fetch has a 10 s connect timeout**, too short for Open-Meteo here (`battletime weather` fails "fetch failed"); preload undici's `setGlobalDispatcher(new Agent({ connect: { timeout: 60000 } }))` via `NODE_OPTIONS=-r`.
