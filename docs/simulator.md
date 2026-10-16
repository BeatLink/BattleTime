# Simulator

BattleTime's simulator runs the real drivers on a desktop, in a 240x240 window, so screen and input
code can be tried without flashing the watch. It follows the idea of InfiniTime's
[InfiniSim](https://github.com/InfiniTimeOrg/InfiniSim), but works differently, because BattleTime is
Rust.

## Running it

```sh
nix run                        # or, inside nix develop: cargo run -p battletime-simulator
nix run . -- --page 2          # start on another page
nix run . -- --scale 3         # a bigger window
nix run . -- --screenshot out.png --page 3   # save one frame and exit, with no window
```

| Input | Does |
|---|---|
| Drag up or down, or press Up and Down | Slide to the next or previous page |
| Space | Press the side button, which gives one tap of the motor |
| R | Ring the motor once |
| S | Save a screenshot as `battletime-<time>.png` |
| Q or Escape | Quit |

A red border shows while the motor is running.

## Running an app

The simulator runs an app image through `battletime-app-host`, the same code the watch will use:

```sh
cargo run -p battletime-simulator -- --app path/to/app.ntapp   # an image file
cargo run -p battletime-simulator -- --demo                    # the built-in stopwatch
nix run . -- --demo --click 120,138 --screenshot out.png    # click Start, save one frame, exit
```

The built-in stopwatch is written with the runtime's image writer (`app-host/src/demo.rs`), so it
runs before the compiler exists. `--click X,Y` taps the screen before a screenshot and may be
given more than once. `--files FOLDER` stands a folder in for the watch's storage, so an app reading
`/remote/buttons` reads `FOLDER/remote/buttons`, and the Files app lists the folder.
`--installed path/to/app.ntapp`, given once for each, lists an image as installed, so the launcher
shows it. `--library path/to/shared.ntlib` gives the app the shared library, for its font and
functions, and the icons file the compiler wrote beside it, `shared.icons`, which the simulator
serves at `/system/icons` as the companion puts it on the watch; `--icons PATH` names another:

```sh
cargo run -q -p battletime-compiler -- faces/digital/face.tsx -o face.ntapp
cargo run -q -p battletime-compiler -- library -o shared.ntlib
nix run . -- --app face.ntapp --library shared.ntlib
```

| Input | Does |
|---|---|
| Click | A tap; held for half a second, a long press |
| Drag | Drag events while the button is down |
| Drag 40 pixels or more | A swipe in that direction |
| Space | The side button, which goes back a screen |
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
cargo run -p battletime-simulator -- --script tests/scripts/stopwatch_start_and_pause.battletime
```

It prints each step as `ok    line 6: button`, and stops at the first that fails with its line,
what it expected and what it found. It exits with 0 when every step passes, 1 when one fails, and
2 when the script cannot be read. A relative `screenshot` path is saved under the current folder.
[testing.md](testing.md) lists the commands.

## Streaming to another program

`--stream` runs an app with no window, for the App Builder
([tools/appbuilder](../tools/appbuilder/README.md)), which shows the screen in VS Code:

```sh
cargo run -p battletime-simulator -- --app app.ntapp --library shared.ntlib --stream
```

It reads one command a line on standard input: `press X Y`, `move X Y` and `release X Y` touch the
screen as the mouse does in the window, `button` presses the side button, `theme` switches theme,
and `quit` or the end of the input stops it. Each time the screen changes it writes
`frame <base64>`, the 240x240 screen row by row as little-endian RGB565, and `motor on` or
`motor off` when the motor starts or stops. Every other line is the usual log text, such as effects.

## How it works

The drivers talk to hardware only through `embedded-hal` traits and `battletime-display`'s
`PanelInterface`, so the simulator hands them simulated parts in place of the chip's. Every driver
in `drivers/` is listed here, with its part or why it has none:

| Driver | In the simulator |
|---|---|
| `battletime-display` (ST7789) | A model of the panel (`simulator/src/panel.rs`), below. |
| `battletime-vibrator` | A pin that runs the motor while low, as P0.16 does; the window shows a red border while it runs. |
| `battletime-heart-rate` (HRS3300) | A model of the sensor (`SimulatedHeartRateChip` in `simulator/src/hardware.rs`) on a wrist with a steady 72 beats a minute. |
| `battletime-touch` (CST816S) | No chip model: the mouse stands in for the finger, and the simulator turns presses, moves and drags into taps, long presses, drags and swipes itself. |
| `battletime-button` | No pin model: Space presses the side button. |
| `battletime-accelerometer` (BMA421 or BMA425) | None yet: steps and motion come from the stand-in system's fixed values. |
| `battletime-external-flash` (XT25F32B) | None: `--files FOLDER` stands in for the flash store above it. |
| `battletime-bluetooth` | None: there is no radio. Its own tests run the link layer and TrouBLE against a simulated central instead; see [bluetooth.md](bluetooth.md#tests). |
| `battletime-status-screen` | Not run in the window, but its tests draw the bootloader's status page through the panel model's SPI wires. |
| `battletime-battery-reading` | None: only the bootloader, its updater and the reloader use it, and none of them runs here. PineTimeEmulator's battery and SAADC stand in for it in the emulator suites. |

- **The screen** keeps the ST7789's 240x320 memory and follows the commands the driver sends, as
  the datasheet describes them: the address window, memory writes, the scroll area and scroll
  start, sleep, idle mode and colour inversion. In idle mode it shows each pixel in the nearest of
  the 8 colours the panel then shows, so low-power screens can be checked here. Each frame, the
  window shows the 240 rows the panel would show. The PineTime's glass inverts colours, so the model
  does too, and a driver that forgot to turn inversion on would show wrong colours here as it would
  on the watch.
- **The motor's** vibrator driver runs on its own thread, so a pattern waits without freezing the
  window.
- **The heart rate sensor's** pulse is shaped like the emulator's. The heart rate driver and its
  `Monitor` run on their own thread (`simulator/src/heart_rate.rs`), 24 samples a second while an
  app has started the sensor, and each reading reaches the app as on the watch, so the Heart app
  settles and then shows 72. The background interval and the beacon's flashing are not simulated;
  the effects are printed.
- **Delays** sleep the thread.

An app runs through the display driver over the panel model, so its scrolled lists move with the
hardware scroll and its transitions draw one step a frame, paced by the clock as on the watch. Without an app, the
simulator shows three demo pages (`simulator/src/pages.rs`), which slide 16 rows a step.

The simulator's tests (`simulator/src/tests.rs`) check the display driver against the panel model,
including that a full slide leaves exactly the frame that drawing the page directly would. They
also run the stopwatch through the display driver, and check how mouse movements become touches.

## Why not InfiniSim itself

InfiniSim is not an emulator. It compiles InfiniTime's C++ source against stand-ins for the
nRF52 and FreeRTOS headers and an LVGL screen drawn with SDL. It cannot run Rust code or a
firmware image, so it cannot run BattleTime.

It is still useful as a reference for InfiniTime's UI, which BattleTime borrows from, so the flake
builds it:

```sh
nix run .#infinisim
```

That builds the InfiniTime version InfiniSim pins, not the `InfiniTime/` checkout beside this repo.

## Limits

- Nothing runs on the nRF52832: timing, RAM use and flash use on the watch are not simulated.
- There is no Bluetooth, touch controller, accelerometer, battery or external flash model, so
  their drivers' own code does not run here; the table above says what stands in for each.
- For the whole firmware on a model of the chip, use the emulator suites in [testing.md](testing.md).
