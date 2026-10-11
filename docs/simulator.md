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

## How it works

The drivers talk to hardware only through `embedded-hal` and `mipidsi` traits, so the simulator
hands them simulated parts in place of the chip's:

- **The screen** is a model of the ST7789 (`simulator/src/panel.rs`). It keeps the panel's
  240x320 memory and follows the commands the driver sends, as the datasheet describes them:
  the address window, memory writes, the scroll area and scroll start, sleep, and colour
  inversion. Each frame, the window shows the 240 rows the panel would show. The PineTime's glass
  inverts colours, so the model does too, and a driver that forgot to turn inversion on would
  show wrong colours here as it would on the watch.
- **The motor** is a pin that runs the motor while low, as P0.16 does. The vibrator driver runs on
  its own thread, so a pattern waits without freezing the window.
- **Delays** sleep the thread.

There is no UI yet, so the simulator shows three demo pages (`simulator/src/pages.rs`). Moving
between them uses the display driver's hardware scroll the way page slides on the watch will: each
step scrolls 16 rows and draws only the 16 rows that have just come on screen. The demo pages are
replaced once the UI toolkit exists.

The simulator's tests (`simulator/src/tests.rs`) check the display driver against the panel model,
including that a full slide leaves exactly the frame that drawing the page directly would.

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
