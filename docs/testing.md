# Testing apps

`neotime-app-testing` in `harness/` runs apps and faces on a test watch, in the spirit of
wasp-os's `make check`. It compiles each one from its source with `neotime-compiler`, opens the
image on the real `neotime-app-host`, and drives it the way the watch would. Nothing is simulated
above the host: the bytecode, bindings, layout and drawing are the watch's own.

```sh
cargo test -p neotime-app-testing      # inside nix develop; cargo test --workspace runs it too
```

Like the simulator, it needs `std`, so it is a workspace member but not a default member.

## Writing a test

A test opens an app by its folder name, drives it, and checks what it shows and asks for:

```rust
use std::time::Duration;
use neotime_app_testing::{EffectKind, TestWatch};

#[test]
fn the_stopwatch_counts_up_after_starting() {
    let mut watch = TestWatch::open_app("stopwatch").unwrap();
    watch.press_button();
    watch.advance(Duration::from_secs(3));
    assert!(watch.shows_text("00:03"));
    assert_eq!(watch.effects_of(EffectKind::KeepScreenOn).last().unwrap().number(0), 1);
}
```

`open_app` and `open_face` read `apps/<id>/app.tsx` and `faces/<id>/face.tsx`; a source that does
not compile fails with the compiler's messages. `open_source` compiles source written in the test
itself, and `open_image` opens an image already built. The shared library is built once for all
tests and given to every app.

| Drive | Does |
|---|---|
| `tap(x, y)`, `long_press`, `double_tap` | A touch as the panel reports it: press, release and the gesture. |
| `tap_text("Dismiss")` | Taps the middle of the shown widget with exactly that text, or fails listing the texts shown. |
| `tap_widget(index)`, `tap_all_over(index, n)` | Taps a widget by its number, or at n by n points across it, such as every key of a keypad. |
| `drag(from, to)`, `swipe(Direction::Up)` | Drag reports between press and release; a swipe ends with the swipe itself. |
| `press_button()`, `long_press_button()` | The side button. |
| `advance(Duration)` | Moves the clock on, waking the app at each time it asked for on the way, in order. |
| `deliver(PushedEvent::Steps { .. })` | An event from the phone or a sensor, with the system values it carries. |
| `receive_from_phone(line)` | A companion line from the phone, read by `neotime-companion` as the watch reads it. |
| `set_system_value`, `set_system_text`, `screen_off`, `screen_on`, `install_apps(&["timer"])` | Change the battery, the firmware version or another value, the screen, or the launcher's list. |
| `write_file(path, bytes)`, `reopen()` | Write a file as the phone does, and leave the app and open it again, so it reads the file afresh. |

| Inspect | Gives |
|---|---|
| `visible_texts()`, `shows_text("…")` | The text of every widget that can be seen; whether one contains the text. |
| `screen_name()`, `tappable_widgets()`, `visible_area(index)` | The open screen and what on it answers taps. |
| `effects()`, `effects_of(kind)`, `take_effects()` | Every effect the app asked for, with text arguments as text. |
| `phone_lines()` | Every line the app's effects sent the phone, as the firmware builds them, such as `send_to_phone`'s `app` line. |
| `failures()` | Every `AppFailure` of the app's code. |
| `frame()`, `pixels_left_stale()` | The drawn 240x240 RGB565 screen; how far drawing only the changes strays from drawing afresh. |

The fake system starts at 09:00 UTC on Monday 5 January 2026, with plausible readings such as 87%
battery, and gives the same random numbers on every run.

## Scripts

A script drives the same test watch from a small text file, one command per line, so an
end-to-end run needs no Rust. `#` starts a comment, blank lines are skipped, and text goes in
double quotes, where `\"` is a quote and `\\` a backslash:

```text
# The stopwatch counts up once started and holds its time while paused.
open app stopwatch
button                      # start
wait 3s
expect text "00:03"
expect effect keep-screen-on true
button                      # pause
wait 1min
expect text "00:03"
expect no failures
```

| Command | Does |
|---|---|
| `open app <id>`, `open face <id>` | Compiles and opens `apps/<id>` or `faces/<id>`; a script starts with one, and may open another later. |
| `install <id> ...` | Installs these apps, so a launcher lists them. |
| `tap X Y`, `tap "text"` | Taps a point, or the middle of the shown widget whose text is exactly that. |
| `long-press ...`, `double-tap ...` | The same, held or tapped twice. |
| `drag X1 Y1 X2 Y2` | Presses, drags in steps and lifts. |
| `swipe up`, `down`, `left`, `right` | A swipe across the middle of the screen. |
| `button`, `long-button` | Presses or holds the side button. |
| `wait 250ms`, `3s`, `10min`, `2h` | Moves the clock on, waking the app at each time it asked for. |
| `set <system value> <number>` | Sets a value by the name apps read, such as `battery_percent` or `settings.step_goal`; `true` and `false` are 1 and 0. As on the watch, `steps_today` also sends the `steps` event and `is_phone_connected` the `phone` event. |
| `notify "title" "body"` | Delivers a notification from the phone. |
| `phone "line"` | Reads a companion line as the watch does, such as `phone "GB({\"t\":\"weather\",\"temp\":291,\"loc\":\"Test City\"})"`, and passes its call, music or weather to the app; the music and weather then replace the fake system's. A line the watch cannot read fails the step. |
| `shake` | Delivers a shake from the accelerometer. |
| `acceleration X Y Z` | Delivers an accelerometer reading in thousandths of g, which `acceleration` subscriptions get at their next period. |
| `screen off`, `screen on` | Turns the screen off or on. |
| `expect text "…"`, `expect no text "…"` | Whether some shown widget's text contains this. |
| `expect screen <name>` | The open screen, by its name in `screens`. |
| `expect effect <name> [arguments]` | An effect by the name apps call it, such as `vibrate 30`, `vibrate-pattern alarm` or `open-app timer`. |
| `expect no effect <name> [arguments]` | No such effect. |
| `expect no failures` | No code of the app has failed. |
| `screenshot <path.png>` | Saves the 240x240 screen as a PNG. |

Names take hyphens or underscores alike. An effect check gives only the arguments it cares
about, and looks at the effects asked for since the step after the previous effect check, so
`expect no effect vibrate` after a tap is not met by an earlier vibration.

A script stops at the first step that does not pass, and reports its line, the command, what it
expected and what it found, such as the texts shown. A line the parser cannot read is named
before anything runs. In Rust, `parse_script` or `load_script` reads a script and `run_script`
plays it, giving a `ScriptRun` with the steps that passed and the `StepFailure`, if any;
`run_script_on` plays one on a watch already open, such as one with an app written in a test.

Example scripts live in `tests/scripts/*.neotime`, and `harness/tests/scripts.rs` plays every one,
so `cargo test` and CI run them. The simulator plays one from the command line (see
[simulator.md](simulator.md)):

```sh
cargo run -p neotime-simulator -- --script tests/scripts/timer_rings_and_dismisses.neotime
```

## The suites

- `harness/tests/smoke.rs` finds every app and face, opens it, taps everything that answers taps,
  swipes four ways, presses the button, moves the clock, delivers one of every phone and sensor
  event, and checks that no code failed and that every frame matches drawing the screen afresh.
  It names each package with a problem, so one broken app is easy to find.
- `harness/tests/quality.rs` checks each package's id against its folder and the id rules, its
  name and launcher icon, its listing in [apps.md](apps.md) or [faces.md](faces.md), the comment
  at the top of its source, the install checks, and that it fits in RAM: its expressions and
  functions within
  the 12 KB the open app has, beside the runtime's working memory, and its heap need within the
  budget ([memory-map.md](memory-map.md#how-much-heap-an-app-needs)), with its opening screen
  building no more nodes than the estimate counts, and every package still fitting, the launcher
  included, with all of them turned on. Packages that break a rule today are listed in `KNOWN_GAPS`
  with the rule they break, and the test fails once one is fixed, so the list only shrinks.
- `harness/tests/behaviour.rs` checks what some apps do, such as the timer ringing when it ends.
- `harness/tests/parity.rs` records the texts each app and face shows in
  `harness/snapshots/texts/<kind>-<id>.json`, described under [Snapshots](#snapshots).
- `harness/tests/scroll_bars.rs` checks scroll bars and the ways a list scrolls: down or across, smoothly or a page at a time, drawn bit by bit as a fresh draw would.
- `harness/tests/screenshots.rs` compares each main screen, and a few screens after some taps,
  with the reference pictures in `harness/snapshots/screens/`.
- `harness/tests/scripts.rs` plays every script in `tests/scripts`.

## Snapshots

Committed snapshot files hold what the watch shows, so the watch tests and the browser preview's
tests share one truth and neither depends on the other running first. Every snapshot is taken on
the fake system above: 09:00 UTC on Monday 5 January 2026, 87% battery and the rest.

### Preview and watch parity

`harness/tests/parity.rs` opens every app and face and writes the texts of its main screen, sorted,
to `harness/snapshots/texts/<kind>-<id>.json`, such as `app-stopwatch.json`. It then makes a few
gestures on a freshly opened copy: a tap on each text shown once, a tap and a long press in the
middle, and the four swipes. Each other screen one of them opens is recorded too, with the gesture
that reached it in `reached_by`.

`apps/tests/parity.test.tsx` and `faces/tests/parity.test.tsx` read the same files. For each screen
they open the app in the browser preview at the same moment, with every system value set as the
fake system sets it and the preview's time zone in UTC, make the same gesture, and compare the
texts. The shared code, and the one list of accepted differences, live in
`sdk/neotime/tests/parity.tsx`. `KNOWN_DIFFERENCES` gives each real difference with a one-line
reason. A listed difference that goes away fails the test too, so the list only shrinks.

The preview test leaves out what the test watch cannot read: an icon's symbol and the keys of a
`ButtonMatrix`, which the watch draws inside the one widget.

### Reference screenshots

`harness/tests/screenshots.rs` draws each app's and face's main screen, plus the stopwatch three
seconds after starting, the calculator after 7 + 5 =, the launcher listing four apps and the remote with 1, 4 and 9 buttons, and
compares the 240x240 frame pixel for pixel with `harness/snapshots/screens/<name>.png`.
[apps.md](apps.md) shows the apps' pictures in its Screen column, so a new app's row there needs
its picture too.

### Reading a failure

- A text snapshot that differs prints the lines that changed: `-` for what the committed file
  holds and `+` for what the watch shows now.
- A preview parity failure shows `watch_only` and `preview_only`: the texts only one side shows,
  beside the known differences expected for that screen. A wrong screen name means the gesture
  in `reached_by` opened a different screen in the preview.
- A screenshot that differs gives the number of differing pixels and leaves two pictures in
  `target/snapshot-failures/`: `<name>.actual.png`, the screen as drawn now, and
  `<name>.difference.png`, the reference dimmed with every changed pixel in magenta. CI keeps
  that folder as the `snapshot-failures` artifact of a failed run.

### Updating snapshots

When a change to what an app shows is meant, rewrite the snapshots and commit them with it:

```sh
NEOTIME_UPDATE_SNAPSHOTS=1 cargo test -p neotime-app-testing --test parity --test screenshots
npm test   # the preview must agree with the new texts, or list why it does not
```

Look at the changed PNGs before committing them. A new app or face needs its snapshots written
this way before the preview's parity test passes.
