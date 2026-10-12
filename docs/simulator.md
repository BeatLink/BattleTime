# Simulator

NeoTime's simulator runs the real drivers on a desktop, in a 240x240 window, so screen and input
code can be tried without flashing the watch. It follows the idea of InfiniTime's
[InfiniSim](https://github.com/InfiniTimeOrg/InfiniSim), but works differently, because NeoTime is
Rust.

## Running it

```sh
nix run                        # or, inside nix develop: cargo run -p neotime-simulator
nix run . -- --page 2          # start on another page
nix run . -- --scale 3         # a bigger window
nix run . -- --screenshot out.png --page 3   # save one frame and exit, with no window
```

| Input | Does |
|---|---|
| Drag up or down, or press Up and Down | Slide to the next or previous page |
| Space | Press the side button, which gives one tap of the motor |
| R | Ring the motor once |
| S | Save a screenshot as `neotime-<time>.png` |
| Q or Escape | Quit |

A red border shows while the motor is running.

## Running an app

The simulator runs an app image through `neotime-app-host`, the same code the watch will use:

```sh
cargo run -p neotime-simulator -- --app path/to/app.ntapp   # an image file
cargo run -p neotime-simulator -- --demo                    # the built-in stopwatch
nix run . -- --demo --click 120,138 --screenshot out.png    # click Start, save one frame, exit
```

The built-in stopwatch is written with the runtime's image writer (`app-host/src/demo.rs`), so it
runs before the compiler exists. `--click X,Y` taps the screen before a screenshot and may be
given more than once. `--library path/to/shared.ntlib` gives the app the shared library, for the
icons apps take from it:

```sh
cargo run -q -p neotime-compiler -- faces/digital/face.tsx -o face.ntapp
cargo run -q -p neotime-compiler -- library -o shared.ntlib
nix run . -- --app face.ntapp --library shared.ntlib
```

| Input | Does |
|---|---|
| Click | A tap; held for half a second, a long press |
| Drag | Drag events while the button is down |
| Drag 40 pixels or more | A swipe in that direction |
| Space, L | Press or hold the side button |
| T | Switch between the default theme and a light one |
| Q or Escape | Quit |

The clock is the computer's. A stand-in system gives fixed values for everything else, such as
87% battery, a weather place and a playing track, and prints each effect the app asks for, such as
`effect: Vibrate(30)` or `effect: VibratePattern(alarm)`, with text arguments as text and named
numbers by name. A
vibration also taps the simulated motor. A failing handler is printed too, where the watch would
show its crash screen. A touch or button press the app does not take is printed, because there
the system's own meaning would apply.

For automated checks of apps without a window, see [testing.md](testing.md).

## Running a script

`--script` plays a test script without a window, through the same test watch `cargo test` uses:
it compiles the apps from their source, so run it from the repository.

```sh
cargo run -p neotime-simulator -- --script tests/scripts/stopwatch_start_and_pause.neotime
```

It prints each step as `ok    line 6: button`, and stops at the first that fails with its line,
what it expected and what it found. It exits with 0 when every step passes, 1 when one fails, and
2 when the script cannot be read. A relative `screenshot` path is saved under the current folder.
[testing.md](testing.md) lists the commands.

## How it works

The drivers talk to hardware only through `embedded-hal` and `mipidsi` traits, so the simulator
hands them simulated parts in place of the chip's:

- **The screen** is a model of the ST7789 (`simulator/src/panel.rs`). It keeps the panel's
  240x320 memory and follows the commands the driver sends, as the datasheet describes them:
  the address window, memory writes, the scroll area and scroll start, sleep, idle mode and colour
  inversion. In idle mode it shows each pixel in the nearest of the 8 colours the panel then
  shows, so low-power screens can be checked here. Each frame, the window shows the 240 rows the panel would show. The PineTime's glass
  inverts colours, so the model does too, and a driver that forgot to turn inversion on would
  show wrong colours here as it would on the watch.
- **The motor** is a pin that runs the motor while low, as P0.16 does. The vibrator driver runs on
  its own thread, so a pattern waits without freezing the window.
- **Delays** sleep the thread.

Without an app, the simulator shows three demo pages (`simulator/src/pages.rs`). Moving
between them uses the display driver's hardware scroll the way page slides on the watch will: each
step scrolls 16 rows and draws only the 16 rows that have just come on screen. The demo pages are
replaced once the UI toolkit exists.

The simulator's tests (`simulator/src/tests.rs`) check the display driver against the panel model,
including that a full slide leaves exactly the frame that drawing the page directly would. They
also run the stopwatch through the display driver, and check how mouse movements become touches.

## Why not InfiniSim itself

InfiniSim is not an emulator. It compiles InfiniTime's C++ source against stand-ins for the
nRF52 and FreeRTOS headers and an LVGL screen drawn with SDL. It cannot run Rust code or a
firmware image, so it cannot run NeoTime.

It is still useful as a reference for InfiniTime's UI, which NeoTime borrows from, so the flake
builds it:

```sh
nix run .#infinisim
```

That builds the InfiniTime version InfiniSim pins, not the `InfiniTime/` checkout beside this repo.

## Limits

- Nothing runs on the nRF52832: timing, RAM use and flash use on the watch are not simulated.
- There is no Bluetooth, touch controller, accelerometer, heart rate sensor or battery yet. Each
  arrives as a simulated part when its driver lands.
