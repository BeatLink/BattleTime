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
    watch.tap(178, 200); // the start button
    watch.advance(Duration::from_secs(3));
    assert!(watch.shows_text("00:03"));
    assert_eq!(watch.effects_of(EffectKind::KeepScreenOn).last().unwrap().number(0), 1);
}
```

`open_app` and `open_face` read `apps/<id>/app.tsx` (or the shell's own `shell/<id>/app.tsx`) and `faces/<id>/face.tsx`; a source that does
not compile fails with the compiler's messages. `open_source` compiles source written in the test
itself, and `open_image` opens an image already built. The shared library is built once for all
tests and given to every app.

| Drive | Does |
|---|---|
| `tap(x, y)`, `long_press`, `double_tap` | A touch as the panel reports it: press, release and the gesture. |
| `tap_text("Dismiss")` | Taps the middle of the shown widget with exactly that text, or fails listing the texts shown. |
| `tap_widget(index)`, `tap_all_over(index, n)` | Taps a widget by its number, or at n by n points across it, such as every key of a keypad. |
| `drag(from, to)`, `swipe(Direction::Up)` | Drag reports between press and release; a swipe ends with the swipe itself. |
| `press_button()` | A click of the side button, which goes back a screen. |
| `advance(Duration)` | Moves the clock on, waking the app at each time it asked for on the way, in order. |
| `deliver(PushedEvent::Steps { .. })` | An event from the phone or a sensor, with the system values it carries. |
| `receive_from_phone(line)` | A companion line from the phone, read by `neotime-companion` as the watch reads it. |
| `set_system_value`, `set_system_text`, `screen_off`, `screen_on`, `install_apps(&["timer"])` | Change the battery, the firmware version or another value, the screen, or the launcher's list. |
| `switch_to_app(id)`, `switch_to_face(id)` | Leaves the open app closed with its state, as the shell does, and opens another. Installed and left apps with timers or alarms run when those come due, and one that calls `wake_screen` opens on the screen its `go` named, with the screen lit; `closed_app_ids()` lists those waiting. |
| `write_file(path, bytes)`, `reopen()`, `start_on_screen(name)` | Write a file as the phone does, and leave the app and open it again, so it reads the file afresh, or start it on another screen as the shell does. |

| Inspect | Gives |
|---|---|
| `visible_texts()`, `shows_text("…")` | The text of every widget that can be seen; whether one contains the text. |
| `screen_name()`, `tappable_widgets()`, `visible_area(index)` | The open screen and what on it answers taps. |
| `effects()`, `effects_of(kind)`, `take_effects()` | Every effect the app asked for, with text arguments as text. |
| `phone_lines()` | Every line the app's effects sent the phone, as the firmware builds them, such as `send_to_phone`'s `app` line. |
| `failures()` | Every `AppFailure` of the app's code. |
| `persisted_value(key, index)` | A value the app persists, by the key its saved state uses, such as `sessions.steps`, at an index for a list. |
| `frame()`, `pixels_left_stale()` | The drawn 240x240 RGB565 screen; how far drawing only the changes strays from drawing afresh. |

A script's `snapshot` names its picture after the package open at that step, which a switch or a
closed app that woke may have changed.

The fake system starts at 09:00 UTC on Monday 5 January 2026, with plausible readings such as 87%
battery, and gives the same random numbers on every run.

## Scripts

A script drives the same test watch from a small text file, one command per line, so an
end-to-end run needs no Rust. `#` starts a comment, blank lines are skipped, and text goes in
double quotes, where `\"` is a quote and `\\` a backslash:

```text
# The stopwatch counts up once started and holds its time while paused.
open app stopwatch
tap 178 200                 # the start button
wait 3s
expect text "00:03"
expect effect keep-screen-on true
tap 178 200                 # pause
wait 1min
expect text "00:03"
expect no failures
```

| Command | Does |
|---|---|
| `open app <id>`, `open face <id>` | Compiles and opens `apps/<id>` (or `shell/<id>`) or `faces/<id>`; a script starts with one, and may open another later. |
| `switch to app <id>`, `switch to face <id>` | Leaves the open app as the shell does, closed with its state kept, and opens another; a closed app's timers and alarms go on and can open it again, as on the watch. |
| `install <id> ...` | Installs these apps, so a launcher lists them. |
| `tap X Y`, `tap "text"` | Taps a point, or the middle of the shown widget whose text is exactly that. |
| `long-press ...`, `double-tap ...` | The same, held or tapped twice. |
| `drag X1 Y1 X2 Y2` | Presses, drags in steps and lifts. |
| `swipe up`, `down`, `left`, `right` | A swipe across the middle of the screen. |
| `button` | Clicks the side button, which goes back a screen. |
| `wait 250ms`, `3s`, `10min`, `2h` | Moves the clock on, waking the app at each time it asked for. |
| `set <system value> <number>` | Sets a value by the name apps read, such as `battery_percent` or `settings.step_goal`; `true` and `false` are 1 and 0. As on the watch, `steps_today` also sends the `steps` event and `is_phone_connected` the `phone` event. |
| `notify "title" "body"` | Delivers a notification from the phone; `notify "app" "title" "body"` names its app, which picks its icon. |
| `phone "line"` | Reads a companion line as the watch does, such as `phone "GB({\"t\":\"weather\",\"temp\":291,\"loc\":\"Test City\"})"`, and passes its call, music or weather to the app; the music and weather then replace the fake system's. A line the watch cannot read fails the step. |
| `shake` | Delivers a shake from the accelerometer. |
| `acceleration X Y Z` | Delivers an accelerometer reading in thousandths of g, which `acceleration` subscriptions get at their next period. |
| `heart 72` | Delivers a heart rate reading in beats per minute; `set heart_rate_status 3` first makes it count as measuring. |
| `navigation` | Tells the app the phone's navigation changed; the turn comes from the system values. |
| `write "/path" "text"` | Writes a whole text file, as the phone does. |
| `reopen` | Leaves the app and opens it again with its state kept, so it reads its files afresh. |
| `start on <screen>` | Starts the open app on that screen with nothing to go back to, as the shell opens the battery page or a notification's preview. |
| `screen off`, `screen on` | Turns the screen off or on. |
| `expect text "…"`, `expect no text "…"` | Whether some shown widget's text contains this. |
| `expect screen <name>` | The open screen, by its name in `screens`. |
| `expect effect <name> [arguments]` | An effect by the name apps call it, such as `vibrate 30`, `vibrate-pattern alarm` or `open-app timer`. |
| `expect no effect <name> [arguments]` | No such effect. |
| `expect no failures` | No code of the app has failed. |
| `screenshot <path.png>` | Saves the 240x240 screen as a PNG. |
| `snapshot <name>` | Compares the screen with the reference picture `harness/snapshots/screens/<kind>-<id>-<name>.png`, named after the open app, such as `app-stopwatch-paused.png`. |

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
- `harness/tests/tours.rs` plays each app's tour, described under [App tours](#app-tours).

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

### App tours

Every app and shell screen has a tour, `tests/tours/<id>.neotime`: a script that opens it, visits
each of its screens, pages and states, and takes a `snapshot` of each. Its page, `docs/apps/<id>.md`,
says how to use the app and shows its main screen and every picture the tour takes; the website
gives each app its page from there. `harness/tests/tours.rs` plays every tour and fails when an app
has no tour or page, a tour does not open its own app, or a page leaves out a picture. A change to
what an app shows updates its tour, pictures and page in the same commit.

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

## The emulator

The harness tests apps; the firmware itself runs in PineTimeEmulator, a Rust emulator of the whole
watch. Its suites are in `neotime/tests/emulator`, described in
[updates.md](updates.md#proving-it-on-the-emulator).

### Running suites

- A script runs as fast as the computer allows, not at the watch's speed, and a sleeping chip skips
  straight to its next event. Only the interactive window keeps to the clock.
- One engine takes about one core and 14 MB, so several suites can run at once. Give each its own
  results folder (`pinetime-test -r <folder>`), because Robot writes `output.xml`, `log.html` and
  `report.html` into the current one.
- A scratch script must use only engine commands: an unknown one, even `echo`, leaves the engine
  waiting forever. Run it with a timeout and stdin closed, and check its CPU use if it seems slow.

### Apps installed

`tools/measure_stack.py` starts with erased external flash, so only the help screen shows. For a
real install, build a flash image and pass it with the library:

```sh
cargo run -p neotime-packages --example filesystem_use -- --image flash.bin --no-theme stopwatch=stopwatch.ntapp
python3 -I ../tools/measure_stack.py FIRMWARE_ELF --launcher --flash flash.bin --library shared.ntlib
```

Without `--no-theme` the image carries an all-black theme, so the launcher and apps draw black.
Without the library no text or icons are drawn, so timings and stack use come out far too low.

### Timing and profiling

- The diagnostics counters `DrawMicroseconds` and `DisplaySendMicroseconds`
  (`firmware/src/diagnostics.rs`) split a screen change into drawing and sending to the display.
  Read them before and after a drawing change.
- To profile, sample `sysbus.cpu PC` from the monitor every half millisecond, symbolise the
  addresses with `llvm-addr2line -i` and count the outermost frame, or inlined helpers swamp the result.
- To find the deepest stack path, log writes near the peak address with
  `sysbus AddWatchpointLog <address> 4 Write @file`, or symbolise the stale words left on the
  stack after a run.
- For frame sizes, scan `llvm-objdump` prologues for both `sub sp, #N` and `subw sp, #N`.
- A screen change that resets the watch with "Reset: watchdog" is usually a draw too slow for the
  liveness check. Time `draw_step_at` per step on the computer in a throwaway harness test,
  built with `--release`, before suspecting the shell. To walk real gestures, script the emulator as
  `tools/measure_heap.py` does.

### Bluetooth without BlueZ

`pinetime-engine --bluetooth <link>` gives the emulated watch a controller on a host link.
`../tools/packages_over_hci.py` and PineTimeEmulator's `tools/hci_scan.py --connect` talk to it
directly, without BlueZ or root. Drive the engine from a script that runs `emulation RunFor
"00:00:00.1"` every 100 ms of real time, because a plain script races ahead and quits. The bridge's
ACL buffers are 27 bytes, so a client splits and rebuilds L2CAP packets itself. A file write takes
emulated seconds, so allow many minutes.

`../tools/check_call.py FIRMWARE_ELF [SCREENSHOT]` writes a phone's incoming call line to the Nordic
UART through the bridge and checks that the call app opens, the motor keeps ringing, and the end
line stops it and goes back to the face; it takes under two minutes.

`../tools/check_heart_rate.py FIRMWARE_ELF` ends with a watch on the bridge that subscribes to the
Heart Rate Measurement (0x2A37): nothing is notified and the sensor stays off until a settings line
turns the background interval to always, then each reading arrives as `00` and the pulse, a read
gives the latest, and turning notifications off stops them.

`../tools/check_phone_services.py FIRMWARE_ELF [SCREENSHOT]` plays a phone that serves the Current
Time and Alert Notification services, answering the watch's own ATT requests through the bridge
from a small attribute table. It checks that the watch asks by itself after connecting, turns New
Alert on and writes the control point, keeps the phone's time and zone as UTC, buzzes for an SMS
alert the phone notifies and opens the call app for a call alert; it takes about a minute.

`../tools/check_watch_face.py FIRMWARE_ELF` installs the analog and digital faces and starts the
firmware three times: with no face chosen analog opens, with digital chosen in the saved settings
digital opens, and with an uninstalled face chosen it falls back to analog; it takes about a minute.

`../tools/check_wake_gestures.py FIRMWARE_ELF` installs a face with raise to wake and lower to sleep
turned on and a 5 second screen timeout, then tilts the emulated accelerometer: the screen must light
for a raised wrist after timing out, go dark for a lowered arm before the timeout, and go dark for
two taps on the face; it needs no Bluetooth and takes about a minute.

`../tools/check_firmware_trial.py [DIST [PICTURE_FOLDER]]` starts the bootloader with `dist/`'s
firmware waiting in the update slot over the recovery firmware, lets it swap in on trial, opens the
System app from the launcher and taps **Confirm**, then checks the boot state says booted and a
restart keeps the firmware; in a second run it taps **Roll back** and checks the bootloader brings
the firmware before back. It needs `tools/build_images.sh` run first and takes about three minutes.

`../tools/check_set_clock.py FIRMWARE_ELF [PICTURE_FOLDER]` installs the digital face, the launcher
and Settings, opens Settings from the launcher, scrolls to **Date & time**, moves the year and the
hour on by one and taps **Set**, then checks that the clock kept across a reset reads that local
time at 00 seconds; it needs no Bluetooth and takes about two minutes.
