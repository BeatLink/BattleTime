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
| `set_system_value`, `screen_off`, `screen_on`, `install_apps(&["timer"])` | Change the battery or another value, the screen, or the launcher's list. |

| Inspect | Gives |
|---|---|
| `visible_texts()`, `shows_text("…")` | The text of every widget that can be seen; whether one contains the text. |
| `screen_name()`, `tappable_widgets()`, `visible_area(index)` | The open screen and what on it answers taps. |
| `effects()`, `effects_of(kind)`, `take_effects()` | Every effect the app asked for, with text arguments as text. |
| `failures()` | Every `AppFailure` of the app's code. |
| `frame()`, `pixels_left_stale()` | The drawn 240x240 RGB565 screen; how far drawing only the changes strays from drawing afresh. |

The fake system starts at 09:00 UTC on Monday 5 January 2026, with plausible readings such as 87%
battery, and gives the same random numbers on every run.

## The suites

- `harness/tests/smoke.rs` finds every app and face, opens it, taps everything that answers taps,
  swipes four ways, presses the button, moves the clock, delivers one of every phone and sensor
  event, and checks that no code failed and that every frame matches drawing the screen afresh.
  It names each package with a problem, so one broken app is easy to find.
- `harness/tests/quality.rs` checks each package's id against its folder and the id rules, its
  name and launcher icon, its listing in [apps.md](apps.md) or [faces.md](faces.md), the comment
  at the top of its source, the install checks, and that it fits in RAM: its expressions within
  the 12 KB the open app has, beside the runtime's working memory, and each screen within the
  55 widgets the UI heap holds. Packages that break a rule today are listed in `KNOWN_GAPS`
  with the rule they break, and the test fails once one is fixed, so the list only shrinks.
- `harness/tests/behaviour.rs` checks what some apps do, such as the timer ringing when it ends.
