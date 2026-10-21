# BattleTime repository: notes for Claude

This repository holds both halves of BattleTime, licensed GPL-3.0-or-later:

- [battletime/](battletime/) is the watch: firmware, bootloader, apps, faces and simulator. Run cargo and
  npm for it from inside `battletime/`.
- [companion/](companion/) is the phone and desktop companion app. Run its commands from inside
  `companion/`.
- [docs/](docs/) holds the documentation for both, and [tools/](tools/) the scripts that build,
  measure and package; the tools find `battletime/` themselves, so they run from anywhere.

Paths below are relative to the repository root.

The phone-to-watch protocol is [docs/companion-protocol.md](docs/companion-protocol.md).
A change to it updates the firmware, the doc and the companion together, in one commit where it can.

The two halves are separate Cargo workspaces, so the companion's desktop crates never change how the
watch builds. `nix develop` gives the watch's toolchain and `nix develop .#companion` the
companion's; `nix develop .#companion-android` adds the Android SDK. Each half has its own CI
workflow under `.github/workflows`, run only when its files or the flake change.

## Working alongside other sessions

- Several Claude sessions work in this repository at once, each in its own worktree under `.claude/worktrees`.
- Talk to the other sessions whenever it helps: find them with `ListAgents` and message them with
  `SendMessage`. Agree on an order when one feature depends on another's landing, say which files
  you are about to change when they might be editing them too, and ask before touching a part
  another session owns.
- Work lands by pushing to GitHub's `master`. Run `git fetch origin`, rebase onto `origin/master`,
  rerun the tests, then push without force.
  When `master` has moved folders, rebase with `git -c merge.directoryRenames=true -c merge.renameLimit=20000`.
- After every push to `origin/master`, bring the main checkout up to date with
  `git -C <main checkout> merge --ff-only origin/master`, because the user opens and runs files there.
  Keep it on the `master` branch, never a detached HEAD. If its tree has changes or the merge
  refuses, leave it and tell the user rather than stashing or discarding anything.
- Never name a commit to anyone until it is on `origin/master`.
- Never revert or reformat another session's files. Run `cargo fmt -p <crate>`, not a bare
  `cargo fmt` over a whole workspace.
- `Cargo.lock` is the usual merge conflict; regenerate it rather than hand-merging.
- Before using the real watch, tell the other sessions, so only one connects at a time.

# The watch: battletime/

BattleTime is a Rust firmware for the PineTime that merges the best of InfiniTime and wasp-os. It is its
own product, licensed GPL-3.0-or-later, so code ported from InfiniTime or PinetimeLite is fine.
Start with [README.md](README.md) and [docs/architecture.md](docs/architecture.md); code style is in
[docs/coding-guidelines.md](docs/coding-guidelines.md).

## Every change

- **Run only the tests for the code you changed**, then land it. Never run whole-workspace suites,
  `tools/check_coverage.sh`, workspace clippy, snapshot sweeps or firmware builds for parts you did
  not touch; CI runs those, and fixing what it finds comes after. New tests still go in the same
  commit as the change.
- App behaviour is tested in `battletime/harness/` ([docs/testing.md](docs/testing.md)), not vitest. A
  change to what an app shows rewrites its snapshots with `BATTLETIME_UPDATE_SNAPSHOTS=1` in the same commit.
- Every app has a tour in `battletime/tests/tours/<id>.battletime` that snapshots each of its screens, and
  a page in `docs/apps/<id>.md` with how to use it and those pictures. A new app, or a new or changed
  screen, updates both in the same commit.
- Docs ship with the code, never as a follow-up. A new crate gets its place in
  `docs/architecture.md`; a change to RAM or flash use updates `docs/memory-map.md` and the
  hand-written part of `docs/memory-treemap.html`.
- Open work lives in `docs/todo.md`. When an item is done, remove it in the same change and record
  it on the matching page of `docs/guide/` if the wearer sees it, or in the part's own doc if not.
  Before removing an item, check in the code that it is really done; keep any open part as a
  narrower item.
- Anything to look at (screenshots, galleries, write-ups) goes in `docs/` and lands with the change.
  Reference existing images such as `battletime/harness/snapshots/screens/*.png` so they stay current.
  The PineTime screen is square, 240x240; never draw it round.
- New drivers go in `battletime/drivers/<name>` and get a simulated part in `battletime/simulator/` plus a
  line in `docs/simulator.md`.

## Design rules

- Going back retraces navigation exactly: it revisits every screen passed on the way forward, in
  reverse order and moving the opposite way (face, notifications, quick settings goes back through
  quick settings, notifications, face). The shell records this in `battletime_app_host::way_back`; a new
  way to move forward must record its step, and a new way back must take it.
- A click of the side button always means back, and apps never hear it. Swipes inside an app are
  the app's own; never bring back swipe-right-to-go-back or a swipe that opens Settings.
- An app's pages run left and right in steps, with a wipe and a page bar, never dots. Scrolling up
  and down is only for lists inside a page.
- Prefer a toggle in Settings or quick settings over a new app that only switches one thing.
- The firmware carries only the engine. The notifications and quick settings screens are compiled
  from `battletime/shell/` into `/system/shell` on external flash, and rare native code such as the
  verifier is a module in `/system/modules`. No built-in apps, faces, launcher, icons or fonts,
  and no fallback copies of them. Apps live in `/apps` on external flash; the shared library (its
  font and functions) ships inside the firmware image, after the code, where the image header says.
  Its icons and the accelerometers' configuration live in `/system` on external flash, which the
  companion puts on any watch that lacks them.
- Anything that changes often (notifications, settings, app state) goes on external flash, never
  internal flash.
- Apps are Preact and Signals compiled on the computer to bytecode ([docs/app-runtime.md](docs/app-runtime.md)).
  About 12 KB of RAM per app rules out any JavaScript engine or Wasm on the watch.
- New settings touch `battletime/services/settings`, `battletime/app-host/src/settings.rs`, `battletime/services/companion`, the
  runtime's system values, the compiler and the sdk. The theme stays in `/settings/theme`.
- Pure Rust only; no closed-source blobs and no C where a bug could brick the watch.
- Flash layout changes go in `battletime/boot/src/lib.rs` and `docs/bootloader.md` together.

## Memory budget

- Keep at least 2 KB of stack spare, measured with `tools/measure_stack.py`. Large state goes in a
  `Box` or lives on the stack only while it is used; a big stack frame overflows the watch's stack,
  which shows as a blank screen and then a rollback.
- The full firmware and its shared library share a 480 KB region with about 44 KB free; anything
  read-only and rarely used belongs on external flash, loaded before painting
  ([docs/external-flash-architecture.md](docs/external-flash-architecture.md)).
- CI measures `docs/memory-map.md` with `tools/measure_memory.py`.

## The watch cannot be recovered by wire

There is no SWD probe, so a watch that cannot reach the bootloader is lost. Recovery rests on the
bootloader at 0x0 (rollback of an unconfirmed firmware, a long button hold for the recovery
firmware) and on DFU in the firmware.

- Never write the bootloader pages from firmware, and keep the recovery slot valid.
- Feed the watchdog only from the main loop and only while the side button is up.
- Prove bootloader, reloader and update changes in the emulator suites under `battletime/tests/emulator`
  before any watch, and only flash with the user's go-ahead.
- After a DFU update something must confirm the firmware (writing the time, or any GATT read), or
  the next reset rolls it back.

## Watch traps

- **Reading address 0 is undefined behaviour.** The bootloader sits at the null pointer. Reading it
  through a Rust pointer or a HAL slice lets LLVM silently delete code after it (a 305 KB firmware
  once built at 44 KB). Use asm loads, as `status::BootloaderPages` does, and compare `cargo size`
  with master after touching such code.
- **Dropping an embassy `Output` floats its pin.** Keep every display pin's `Output` alive while the
  screen is in use, or the reset pin drops and the panel goes blank.
- **Big futures and statics land on the stack.** An async fn's future in a static task pool or under
  `block_on` costs its full size, so box it or keep large locals from living across awaits. An async
  fn awaited inside a task merges into the task's frame; poll a big one through an `#[inline(never)]`
  wrapper. `*self = Big::new()` and `StaticCell::init(array)` both build the value on the stack
  first; use `ConstStaticCell` for big statics.
- **A blocking wait in WFE must SEV afterwards**, or the executor sleeps with a task ready.
- **A slow draw resets the watch.** Anything that blocks the shell for a few seconds stops the
  watchdog being fed, so it shows as "Reset: watchdog", not a slow frame. A widget that draws many
  lines must skip those outside `DrawContext::clip`, because every strip redraws it.
- **New bytecode instructions need the analysis taught.** One that writes an array must call
  `note_write` with what it can store, or the compiler picks too narrow an element width.
  `derive_inputs_in` in `battletime/app-host/src/subscriptions.rs` and the bindings must learn every
  instruction that reads or writes state. A library function's declared return type must be
  passed as `expected`, or a `Color` comes back as text.
- **A test compile error can pass unseen.** Grep test output for `^error` as well as `FAILED` and `panicked`.
- **The emulator runs delay loops at the chip's speed**, about 50 million instructions a second, so
  MCUBoot's start-up takes about 20 emulated seconds.
- **`filesystem_use` writes an all-black theme** unless given `--no-theme`, so the launcher and apps
  render black in the emulator. Running the emulator is in [docs/testing.md](docs/testing.md#the-emulator).
- **`tools/measure_stack.py` starts with erased external flash**, so it measures only the help
  screen unless given `--flash` and `--library`. It measures whatever ELF sits at the release path,
  and the recovery build has its own profile, so it lands in `target/thumbv7em-none-eabihf/recovery/`
  instead.
  `--packages` runs `pinetime-engine --bluetooth` and needs a library built from the same commit
  as the firmware, or the firmware faults at start and the watch never connects.
- **The emulator models no chip anomalies.** The SAADC never ends a sample started straight after an
  offset calibration (nRF52832 anomaly 86), so the blocking reader stops the converter in between;
  code that drives a peripheral's registers itself needs a check on the watch, not only the suites.
- **Bootloader builds are not byte-reproducible across checkout paths**; the code is the same.
- **`cargo build -p <dev-dependency>` panics in cargo**; check such a dependency from a throwaway crate.
- **Never `pkill -f` a pattern that also appears in your own command line**; it kills the shell.
- **A `=======` regex for conflict markers must be anchored** to the start of the line, or it
  matches the `// ====` section banners.
- **The InfiniTime checkout beside this repo is a local reference copy.** Never push it.

# The companion: companion/

The companion app for watches running the BattleTime firmware: one Tauri app for the desktop and
Android, and a command line tool. Read [companion/README.md](companion/README.md) for the layout and
the protocol. Paths in this part are relative to `companion/`.

- The window is Preact and TypeScript, built by Vite into `dist/`, which Tauri serves. Screens live in
  `src/screens`, shared pieces in `src/components`, and the routes in `src/app.tsx`. It runs in
  WebKitGTK and in Android's WebView, so lay it out for a phone's width and use only web APIs.
- Bluetooth stays behind `src/ble/transport.ts`; screens use the `useWatch` hook and never reach the
  bridge directly. The window's transport is `src/ble/tauri-transport.ts`, over `window.battletime`
  from `tauri/src/bridge.js`; in a plain browser (`npm run dev`) the mock watch stands in.
- `core/` is the Rust core with no Tauri in it (Bluetooth on btleplug, sync, plugins, web requests);
  `tauri/` is the app, whose `src/bridge.js` is the window's only way to the core. Every step in
  `core/src/ble.rs` has a timeout and a failed connect drops the link, because a half-open link
  makes every later attempt wait for services that never resolve. Verify Rust with
  `cargo test --workspace` and `cargo clippy --workspace --all-targets` in `nix develop .#companion`;
  the Tauri crate needs a `dist/` to build, from `npx vite build`.
- The app builds for Android with `npm run tauri-android` (debug) and `npm run tauri-android:release`
  in `nix develop .#companion-android`, project in `tauri/gen/android`. Its phone bridge is
  `tauri-plugin-phone-bridge`, whose Kotlin services and commands live in its `android/` folder.
  Desktop-only Rust sits behind `#[cfg(desktop)]`.
- The phone's part (notifications, media, Do Not Disturb, location) sits behind `phoneBridge` in
  `src/services/phone`. On the desktop the computer plays it through `tauri/src/desktop.rs` over
  D-Bus; check `phoneBridge.kind`, `supportsCalls` and `supportsLocation`, never the platform.
- Plugins are the phone-side halves of watch apps, installed at run time and run in QuickJS in
  `core/src/plugins`; [docs/plugins.md](docs/plugins.md) is the author's guide and must match what
  the host offers. A plugin may send only the watch messages and fetch only the hosts its manifest
  lists, so a new host function needs a matching permission check and test.
- `cli/` is the command line tool. It imports the same `src/` modules the screens do, so a feature
  gets a command rather than a second implementation, and talks to BlueZ through `node/ble.js`.
  `npm run battletime -- <command>` rebuilds it with esbuild and runs it.
- Raw GATT goes through the `DfuLink` each transport supplies with `dfuLink()`. The DFU controller,
  `src/protocol/battletime.ts` (time, battery, diagnostics, capabilities) and
  `src/protocol/file-service.ts` all use it and never touch a Bluetooth library.
  `src/dfu/mock-legacy-target.ts` is the mock watch behind it, BattleTime or wasp-os.
- A feature is offered only when the watch serves what it needs. `useWatch` gives `watchReady`
  (connected and running firmware) and `capabilities` (`dfu`, `time`, `battery`, `diagnostics`,
  `files`, `uart`); use those, not `connection`, to decide whether a feature is shown.
- `src/protocol/battletime.ts`, `file-service.ts`, `gadgetbridge.ts` and `requests.ts` mirror
  [docs/companion-protocol.md](docs/companion-protocol.md); change it there first, then here. The
  Rust core carries its own copies as it takes the protocols over: `core/src/protocol/battletime.rs`,
  `packages.rs` and `app_image.rs`, and `core/src/gadgetbridge.rs`; a protocol change updates both
  until the TypeScript one is gone.
- `src/bundles/watch-apps.ts` is generated by `npm run import-watch-apps ../battletime/dist/store` from
  the folder `tools/build_store.py` fills ([docs/app-store.md](docs/app-store.md)); do not edit it by
  hand. Rebuild it whenever the watch's widget or instruction numbering changes, or an app is added.
- After a firmware update the app must exchange GATT data with the watch (it writes the time), or
  BattleTime rolls the update back at its next reset.
- Use `nix develop .#companion` for the toolchain, and `nix develop .#companion-android` for the
  Android SDK. A change to `package-lock.json` needs a new `npmDepsHash` in the repository's
  `flake.nix` (`nix run nixpkgs#prefetch-npm-deps -- package-lock.json`).
- Open work is in [companion/TODO.md](companion/TODO.md); remove an item there in the same change
  that finishes it.
- Every change ships with tests in the same commit. Run only the test files for what you changed
  (`npx jest <path>`); CI runs `npm run coverage`, typecheck, lint and the Vite build whenever
  `companion/` changes. A test that needs the DOM starts with a `@jest-environment jsdom` docblock.

## Companion traps

- **A BlueZ link can wedge.** `bluetoothctl info` says `Connected: yes` with no RSSI, and
  `disconnect` and `remove` both hang. Only switching the adapter off and on clears it, which drops
  every Bluetooth link on the computer, so ask the user first.
- **Install `node_modules` in your own worktree.** Run `npm ci` in `nix develop .#companion` (about
  30 s); never link another worktree's, because its session can remove that worktree mid-task.
- **Preact is ESM only.** Jest transforms it through Babel (`transformIgnorePatterns` in
  `package.json`); a new ESM-only dependency needs adding there too.
- **jsdom lacks some web APIs**, such as `TextEncoder` and `Blob.arrayBuffer`, that WebKitGTK and
  Android's WebView have; a test fakes them rather than the code avoiding them.
- **Node's fetch gives up on slow hosts.** Its 10 s connect timeout is too short for Open-Meteo from
  this machine, so a CLI command such as `battletime weather` fails with "fetch failed". Preload
  undici's `setGlobalDispatcher(new Agent({ connect: { timeout: 60000 } }))` through `NODE_OPTIONS=-r`.
