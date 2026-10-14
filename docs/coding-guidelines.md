# Coding guidelines

NeoTime code should read plainly. Make it readable by how it is named and laid out, never by
cutting features.

## Names

- Use full snake_case words: `duration_in_milliseconds`, `motor_pin`, `first_memory_row`.
- Type parameters are full words too: `MotorPin`, `PwmChannel`, `DisplayInterface`.
- No single letters, abbreviations (`di`, `rst`, `y`) or tuple fields like `self.0`. Use a small
  named struct instead of a tuple.
- Put units in the name when there is one: `_in_milliseconds`, `_percent`, `_rows`.
- Give magic numbers a name: `const SET_SCROLL_START: u8 = 0x37;`.

## Layout

Split each file into sections, each under a banner:

```rust
// ============================================================================
// Patterns
// ============================================================================
```

Inside a function, leave a blank line between setup, action and checks.

Prefer a standard helper over a hand-rolled trick, for example `embassy_futures::block_on` or
`core::mem::take`.

## Comments

- Let names carry the meaning, so most items need no comment.
- A comment is one sentence on one line, in plain words.
- Only the overview at the top of a module may run longer, to say what the module is for.
- A comment says what the code is and does now, never what it used to be; one short sentence may
  name a hazard, such as why a read of address 0 uses assembly.
- Explanations and measurements go in `docs/`, not in the code.

## Drivers

- Each driver is its own `no_std` crate under `drivers/`, named `neotime-<part>`.
- A driver talks to hardware only through the `embedded-hal` traits, never a chip crate, so it
  can run in tests and the simulator. It may build on a portable crate such as `mipidsi`. The only
  exceptions are parts inside the nRF52832 itself, which `embedded-hal` has no traits for: the
  Bluetooth radio keeps all its register access in one file behind a small trait
  ([bluetooth.md](bluetooth.md)), and `neotime-battery-reading` drives the SAADC for programs with
  no executor.
- A new driver gets a simulated part in `simulator/` and a line in [simulator.md](simulator.md).
- Tests live in `src/tests.rs` beside `src/lib.rs`, or in `src/<module>_tests.rs` beside a module,
  and drive the code through fake hardware that records what it was sent.
- Name tests after the behaviour they check: `splits_where_memory_wraps`,
  `cancelling_a_vibration_turns_the_motor_off`.

## Apps and shared parts

- Apps are assembled from shared parts that live in the firmware and the shared library; the app
  itself holds only what is unique to it. RAM is the scarcest resource, and shared parts cost
  none. [app-runtime.md](app-runtime.md#building-apps-from-shared-parts) gives the rules.
- Use the shared parts before writing your own, and move anything a second app needs into the
  shared library, written generically.
- Build a composite widget with its own logic, or heavy work over a whole array, natively in the
  firmware rather than in app code.
- Never put an app or a face in the internal flash.

## Tests and coverage

- Every change ships with tests for what it adds or changes, in the same commit.
- Run only the tests for the code you changed. CI runs the rest on every push and pull request:
  every workspace's tests, clippy, the firmware builds, the emulator suites and
  `tools/check_coverage.sh`, which fails below the coverage floors (the Rust one in the script, the
  TypeScript ones in `vite.config.ts`).
- When a change raises coverage, raise the floor in the same change, so it never slips back.
- App behaviour is tested in `harness/`, with snapshots of each screen; see [testing.md](testing.md).
- The firmware, bootloader and reloaders build only for the watch; keep their logic in workspace
  crates where tests can reach it, and test the rest in the emulator suites under `tests/emulator`.

## Formatting and checks

- Run `cargo fmt -p <crate>` for the crates you changed (maximum line width 110, from
  `rustfmt.toml`), never a bare `cargo fmt` over the whole workspace.
- A change to firmware code also builds for the watch, `cargo build --release --target
  thumbv7em-none-eabihf` in `firmware/`, and keeps 2 KB of stack spare, measured with
  `tools/measure_stack.py`; a change to RAM or flash use updates [memory-map.md](memory-map.md).

## Commits

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):
`<type>(<scope>): <description>`, for example `feat(display): ...`.
