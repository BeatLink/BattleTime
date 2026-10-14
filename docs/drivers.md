# Drivers

The drivers in `neotime/drivers/` each speak to one part of the PineTime. They reach it through the
`embedded-hal` traits and never touch the chip directly, so the same driver runs on the watch, in
tests on a computer and in the [simulator](simulator.md). The one exception is the Bluetooth
controller, because the radio is inside the chip; it keeps all register access in one file behind a
small trait.

Each driver is a `no_std` crate with its code in `src/lib.rs` and its tests in `src/tests.rs`. The
[firmware](firmware.md) owns the chip's buses and pins and hands them to the drivers; how the parts
are wired is in [architecture.md](architecture.md#the-hardware).

## Display (`drivers/display`)

`neotime-display` drives the ST7789 screen with its own start-up, window, scroll and power
commands. It reaches the panel through `PanelInterface`: commands, and pixels as bytes in the
panel's order, high byte first. `MipidsiInterface` adapts any
[`mipidsi`](https://crates.io/crates/mipidsi) interface, such as its `SpiInterface`, which the
reloaders use. On top it adds what the PineTime needs:

- **Strips sent in the background.** `send_strip` points the panel at a rectangle, split where
  memory wraps, and hands the interface panel pixels to send there, which it may still be doing
  after it returns, such as by DMA; the caller leaves them untouched until the next call.
  `neotime-ui`'s `BufferedScreen` pairs a display with two strip buffers and alternates them.
- **Hardware vertical scrolling.** `scroll(offset)` moves the picture with the panel's own scroll,
  which costs one short command instead of a redraw. `set_scroll_area(top, bottom)` keeps fixed
  areas, such as a status bar, still. The app host's slides and list scrolling are built on it.
- **Drawing follows the scroll.** The ST7789 holds 320 rows of memory but shows only 240. After a
  scroll, drawing at a screen row lands on whichever memory row is on screen there, and a drawing
  that crosses the end of memory is split in two.
- **embedded-graphics.** `Display` is a `DrawTarget` for `Rgb565`, so any `embedded-graphics`
  shape, text or image draws on it.
- **Power.** `sleep` and `wake` turn the panel off and on and keep its memory. The panel needs
  6 ms after either before its next command, and 125 ms between the two
  (`SLEEP_SWITCH_GAP_IN_MILLISECONDS`).
- **Low-power mode.** `enter_low_power_mode` switches the panel to its idle mode, as InfiniTime
  does for an always-on screen: only 8 colours, the top bit of each channel, refreshed at a quarter
  of the rate. Memory keeps every colour, so `leave_low_power_mode` shows the screen as drawn, and
  drawing works the same in either mode.
- **Backlight.** `Backlight` drives the three backlight pins, P0.14, P0.22 and P0.23, which each
  light while low. `BacklightLevel` is `Off`, `Low`, `Medium` or `High`, and each level lights one
  more pin; `brighter` and `dimmer` step between them.

The board passes in the SPI interface, the reset pin and a delay.

## Touch panel (`drivers/touch`)

`neotime-touch` drives the CST816S touch panel over I2C at address 0x15. On the PineTime its
interrupt is P0.28 and its reset P0.10, on the I2C bus at SDA P0.06 and SCL P0.07.

- **Reports.** The panel pulses its interrupt pin low when a finger lands, moves or lifts and when
  it sees a gesture, and keeps pulsing while the finger stays down. `wait_for_report` waits for the
  next pulse and reads one `TouchReport`, or `wait_for_pulse` and `read_report` do it in two
  steps, so the caller can time the report by its pulse. A report holds the position in display
  columns and rows, the panel's `Gesture` (tap, long press, a slide in one of four directions, or a
  double tap), whether the finger went down, lifted or is still touching, and whether anything is
  touching at all.
- **Garbled reads are dropped.** A report with a position off the 240x240 panel or an unknown
  gesture is skipped, as InfiniTime does.
- **One finger, with noise and lost reports allowed for.** `Finger` turns reports into landing,
  moving and lifting. A position more than 60 px from the last is ignored unless the next report
  lands near it too, a move while no finger is down lands one, and a second lift is nothing.
  `lift` ends a touch whose lift report never came, at the last position.
- **Gestures come from the finger's path.** The panel's own gestures arrive late and only for long
  straight strokes, so `GestureFinder` works out each touch's one gesture from where and when the
  finger lands, moves and lifts:

    | Gesture | When |
    |---|---|
    | Drag | The finger has moved 10 px. |
    | Swipe | A drag has gone 40 px along one direction; it is reported at once. |
    | Flick | A stroke of 20 px whose finger still moves at 400 px/s that way as it lifts; it is a swipe at the lift. |
    | Long press | The finger has stayed still for 500 ms. |
    | Tap | The finger lifted sooner, after touching for at least 30 ms; a shorter touch is a bounce or a brushing sleeve. |

    The speed is read over the last 80 ms of reports, so a slow drag that ends in a quick toss is a
    flick, and `speed` gives it, capped at 2,500 px/s, for lists to glide on. A diagonal stroke,
    whose main direction is not half as long again as the other, is no swipe.
- **Double taps are off by default.** With them on, the panel holds back every single tap while it
  waits to see if a second follows. `Settings { report_double_taps: true }` turns them on.
- **Sleep.** `sleep` puts the panel into deep sleep, holding it in reset if it does not answer.
  `sleep_until_double_tap` instead keeps it listening for a double tap only, so a double tap can
  wake the watch. `wake` restarts it with the last settings.
- **Resets.** The panel stops answering on I2C while it is idle, so `start`, `sleep` and
  `sleep_until_double_tap` each begin with a reset: reset low for 5 ms, then 50 ms to settle.
  `read_chip_info` works only right after one of these or right after a touch.

The setup matches InfiniTime's: it turns off the panel's automatic reset after five seconds without
a gesture, which would otherwise cut off a long drag. NeoTime has its own driver because `cst816s`
is blocking, and `cst816s-async` reports only gestures, not finger movement, and forces `defmt` on
every user.

## Button (`drivers/button`)

`neotime-button` drives the side button:

- **Power.** The button only reads while P0.15 is high, so `Button::new` drives it high. The board
  passes P0.13 as an input with its pull-down on.
- **Debouncing.** A change of level counts only if it still holds 10 ms later, so contact bounce
  and short glitches are ignored.
- **Gestures.** `next_gesture` reports a `Click`, a `DoubleClick` (a second press within 200 ms of
  letting go), a `LongPress` (held 400 ms) and a `LongerPress` (held 2 s), the same timings as
  InfiniTime. A click is reported only once the double click window closes; callers that want
  presses straight away use `wait_for_press` and `wait_for_release`. Long presses are reported
  while the button is still held, and letting go afterwards is not a click. `Timing` changes the
  thresholds.
- **Recovery.** `is_pressed` reads the pin at once. The firmware's watchdog task reads the pin
  itself and feeds the watchdog only while it is low, so holding the button lets the watchdog reset
  the watch into the bootloader.

The driver keeps no buffers: it uses a few bytes of RAM in the task that owns it.

## Vibrator (`drivers/vibrator`)

`neotime-vibrator` drives the vibration motor:

- **Two kinds of wiring.** `OnOffMotor` uses a plain pin, which is either on or off. `PwmMotor` uses
  a PWM channel, so it can buzz at any `Strength` from 0 to 100 percent.
- **Patterns.** A pattern is a list of `Step`s, each a strength and a duration. `TAP` is one short
  buzz, and `RING` is one second of ringing that is played again to keep ringing.
- **Async and safe to cancel.** `vibrate` and `play` wait with an async delay, so other tasks keep
  running. If a pattern is cancelled part way, the motor is turned off.

## Accelerometer (`drivers/accelerometer`)

`neotime-accelerometer` drives the Bosch accelerometer over I2C at address 0x18, on the same bus as
the touch panel. PineTimes carry either a BMA421 (chip ID 0x11) or a BMA425 (chip ID 0x13).

- **Configuration file.** The chip counts steps with its own small processor, which runs a 6 KB
  program from Bosch. `start` resets the chip, reads its ID, uploads that chip's file from the
  `Configurations` it is given 16 bytes at a time, and fails with `ConfigurationRejected` if the
  chip does not report it ready 150 ms later, or `NoConfiguration` when it is given none for that
  chip. Starting takes a few tenths of a second.
- **Steps.** `start` turns the step counter on. `read_step_count` reads the count since the last
  `reset_step_count`. The count lives in the chip, so the firmware only reads it.
- **Acceleration.** `read_acceleration` gives the latest sample in milli-g, sampled at 100 Hz with
  4-sample averaging. `Settings.range` picks ±2 g (the default), 4, 8 or 16 g.
- **Watch axes.** The chip is turned on the board, so `Orientation::PINETIME` swaps its x and y
  axes and reverses one, the same as wasp-os. `Orientation::CHIP` gives the chip's own axes.
- **Power saving.** The chip runs in Bosch's advanced power saving mode while counting. Its
  feature memory only takes writes with that mode off, so the driver pauses it around each one.
- **Bus watchdog.** The chip's I2C watchdog is on, so a transfer cut off part way frees the bus
  after 40 ms instead of holding it.

Two detectors work in software on the readings, so the chip's own tap and wrist-wear detectors are
not set up:

- **Wake gestures.** `wake_gestures::WakeGestureDetector`, ported from InfiniTime's
  `MotionController`, keeps the last eight readings in InfiniTime's units (1024 to a g, the y axis
  reversed) and finds a raised wrist (`ShouldRaiseWake`), a lowered arm (`ShouldLowerSleep`) and a
  shake faster than the threshold, from InfiniTime's shake speed: an average of how far each
  reading moved from the one before. It judges the roll between the mean of the newest two
  readings and of the oldest two with a whole-degree arcsine table, starts afresh after a gap of
  more than 500 ms, and reports one gesture a second at most.
- **Shakes.** `shake::ShakeDetector` takes readings about ten times a second: a reading more than
  1200 milli-g from the one before, summed over the axes, is a jolt, three jolts within a second are
  a shake, and the next second is ignored, so one shake counts once. Readings more than half a
  second apart are not compared.

The configuration files are not in the firmware: the shared library carries both, as its data 0
and 1 (`LibraryData`), which keeps one image for every watch and saves the firmware 12 KB. The
`configuration-files` feature gives them to the compiler, which puts them in the library. The
firmware's `motion.rs` passes the files from the library inside the firmware to `start`; a build
without a library never starts the step counter and reports diagnostic state 4. The files are
Bosch's, under the BSD-3-Clause licence in `src/configuration/LICENSE`, taken from the
`BMA42X-Sensor-API` that wasp-os uses.

NeoTime has its own driver because the `bma423` crate is blocking, accepts only chip ID 0x13 and
has no step counter.

## Heart rate sensor (`drivers/heart_rate`)

`neotime-heart-rate` drives the Tianyi HRS3300 over I2C at address 0x44, on the bus the touch panel
and the accelerometer share. Its green LED shines into the skin, and the chip measures what comes
back (the pulse channel) beside an ambient light channel.

- **The chip.** `start` checks the ID (0x21), then sets a conversion every 12.5 ms, the LED at
  20 mA, both channels to 16 bits and a gain of one, as wasp-os does, and turns measuring on.
  `stop` turns measuring and the LED off. `read` takes the six data registers, 0x08 to 0x0f, in
  one read, as InfiniTime does. `Command::Flash` lights the LED at one of four drive levels (12.5
  to 40 mA) once every wait time (800 ms down to none) for wasp-os's beacon. All of them go
  through `perform`, so the firmware reaches the chip from one place, which keeps its task small.
- **The pulse (`ppg`).** wasp-os's filter chain, sampled 24 times a second: the first sample's
  level is taken off, then a high-pass filter, a gain control that follows the peaks and drops
  spikes over twice their size, and a low-pass filter leave a signed byte. A window of 200
  samples, about eight seconds, is compared with itself shifted by 7 to 48 samples (205 down to
  30 beats a minute); the first shift where it matches best is one beat, checked again at two,
  three and four beats. The rate comes from the fourth beat, or the third when there is no
  fourth. After each window the newest 104 samples stay, so readings come every four seconds. The
  beat is counted at the best match itself, not one sample past it as wasp-os does, which would
  read about 1% low. On a steady pulse it reads 45 to 180 beats a minute within two.
- **The status (`monitor`).** `Monitor` gives the status apps read in `heart_rate_status`: 2
  (settling) until a window yields a rate, then 3 (measuring), and back to 2 after a window with
  none. As InfiniTime does, each good reading sets an ambient light limit at twice the light
  then; more light than that means the watch left the wrist, so the window starts again and the
  status is 1 (no skin). A reading's confidence is 100% for a rate matched over four beats and 75%
  over three.
- **What it does (`schedule`).** `Schedule::mode` puts the open app first: flashing for a beacon,
  measuring until stopped or for one reading. With no app asking, the heart rate interval setting
  decides, as InfiniTime's does: off, always, or a measurement every 30 seconds, minute, 5, 10 or
  30 minutes. Each lasts until a reading, or gives up after 30 seconds, as off the wrist, and
  the next starts the whole interval later. `flash_request` turns the beacon's brightness and
  period in milliseconds into the chip's drive level and wait time.

## Bluetooth controller (`drivers/bluetooth`)

`neotime-bluetooth` is NeoTime's own Bluetooth LE controller: the half of a Bluetooth stack that
drives the radio. The [TrouBLE](https://github.com/embassy-rs/trouble) host runs on top of it,
connected through `bt-hci`'s `ExternalController` as if it were a separate chip. It replaces
Nordic's closed SoftDevice Controller and saves about 27 KB of flash and 5 KB of RAM.

- **What it does.** Advertising with scan responses, and one connection as the peripheral on the
  1 Mbit PHY, with acknowledgements, flow control, and parameter and channel map updates. There is
  no encryption yet.
- **How it is split.** The HCI interface and the link layer are plain logic that runs in tests on
  a computer, including TrouBLE itself on a fake radio. Only `nrf_radio.rs` touches the chip: the
  radio, TIMER0, the random number generator and five fixed PPI channels.
- **Progress for the watchdog.** `radio_progress` gives the radio's interrupt count and whether it
  has work, which `neotime-liveness` judges, and `check_supervision` ends a connection whose radio
  stopped sending events.
- **Status.** It runs on the watch, and firmware updates over Bluetooth use it. The checks under
  "Hardware tests" in [todo.md](todo.md) have not been made yet.

See [bluetooth.md](bluetooth.md) for the design, the timing, what it takes from the chip and what is
missing. What the watch advertises and the GATT services it serves are in
[firmware.md](firmware.md#bluetooth).

## External flash (`drivers/external_flash`)

`neotime-external-flash` drives the 4 MB SPI NOR flash as an `embedded-storage` `NorFlash`, the
trait embassy-boot and the flash store are reached through:

- **Any 4 MB chip.** `ExternalFlash::new` wakes the chip from deep power-down, since the firmware
  may have left it asleep before a reset, then checks that it answers as a 4 MB chip. Only standard
  SPI NOR commands are used, so a second-source part works as well as the XT25F32B.
- **Writes of any length.** The chip programs at most 256 bytes at a time, within one page, so
  `write` splits at page boundaries. `erase` takes whole 4 KB sectors.
- **Power.** `sleep` puts the chip in deep power-down once any running erase or program ends, and
  `wake` brings it back. The driver remembers putting it down and wakes it before its own next
  command; `wake_if_asleep` is for anything that reaches the chip past the driver, such as the
  firmware's DMA reads. When the firmware lets the chip sleep is in
  [firmware.md](firmware.md#storage).
- **Blocking and async.** While an erase or a write is in progress, both check the chip every 100
  microseconds and wait in between. `ExternalFlash` blocks, which is what the bootloader, the
  reloader and the firmware's update slots need; it waits on a blocking delay, and the watch
  passes `BusyWait` (the `cortex-m` feature), which spins the CPU without touching a peripheral,
  since back-to-back status reads make the emulator spend most of an erase in its SPI model.
  `AsyncExternalFlash` implements the `embedded-storage-async` traits and waits on an async delay,
  so other tasks run. Both share their commands, range checks and page splitting.
- **Without waiting.** `start_erasing_sector` and `start_programming_page` start an erase or a page
  program and return at once, and `operation_has_finished` reads the chip's status once. Until one
  says it has finished, any other call first waits for it. The firmware's flash store polls these
  with the timer between, so other tasks run while the chip works.
- **Rewrites.** Both mark themselves `MultiwriteNorFlash`, since NOR flash can be written again
  without an erase as long as bits only change from 1 to 0.

It keeps no buffers.

## Battery reading (`drivers/battery_reading`)

`neotime-battery-reading` takes one blocking reading of the battery's voltage for the programs that
run before the firmware. The bootloader, its updater and the reloader have no executor and no
interrupts, so they cannot use embassy's SAADC driver. It sets up P0.31 as the firmware does,
calibrates, stops the converter (the nRF52832 never ends a sample started straight after an offset
calibration, anomaly 86), averages four samples and gives the voltage, or none when the converter
never answers. Every wait gives up after a bounded number of polls.

[`neotime-battery`](services.md#battery-servicesbattery) turns that reading into the status page's
battery line and decides whether there is enough charge to write the bootloader: power present and
at least 3.7 V. The tests run the reader against a stand-in for the SAADC's registers. The firmware
itself reads the battery through embassy; see [firmware.md](firmware.md#battery).

## Status screen (`drivers/status_screen`)

`neotime-status-screen` sets out the status page every part of NeoTime shows, and holds a small
ST7789 driver to show it from the bootloader:

- **`page`** says which of the nine lines shows what (the title, the bootloader, the firmware, the
  files, Bluetooth, what is happening, its progress, the battery and the last restart), with their
  labels, font and colours. The firmware, the recovery
  firmware, the bootloader and the bootloader updater all draw through it, so the page looks the
  same whichever of them is running.
- **`Screen`** sends the few commands a status page needs and nothing else, small enough for the
  bootloader's 20 KB.

The crash and memory pages in [`neotime-crash-record`](services.md#crash-record-servicescrash_record)
draw in the same font.
