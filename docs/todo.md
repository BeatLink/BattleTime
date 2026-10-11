# To do

Work that is known to be needed but not done yet.

## Hardware tests

- [ ] **Touch panel on the PineTime.** `neotime-touch` has only run against fake hardware. Once the
  board layer can start a driver, check on the watch that:
  - `start` succeeds and `read_chip_info` gives a sensible chip ID (0xb4 or a near value);
  - taps, long presses and slides in all four directions come through as the right `Gesture`;
  - a drag streams reports while the finger stays down, and the finger lifting is reported;
  - positions match the display, with column 0 and row 0 at the top left;
  - `sleep` stops the interrupts, and `sleep_until_double_tap` wakes on a double tap only;
  - `wake` brings back the settings `start` was given.

- [ ] **Accelerometer on the PineTime.** `neotime-accelerometer` has only run against fake
  hardware. On the watch, check that:
  - `start` succeeds and names the chip the watch really has, which on this watch is a BMA425;
  - lying flat with the screen up reads about +1000 milli-g on z, and tilting it reads the
    expected signs on x and y, which settles whether `Orientation::PINETIME` is right;
  - walking raises `read_step_count`, and `reset_step_count` sets it back to 0;
  - the upload still works with chunks larger than 16 bytes, which would make starting faster.

- [ ] **Bluetooth controller on the PineTime.** `neotime-bluetooth` has only run against a fake
  radio. Once there is firmware to put it in, check on the watch, with a phone and with `btmon` on
  the computer, that:
  - advertisements show up on all three channels, with the right name and address;
  - a scan request gets the scan response;
  - a phone connects, and the connection survives the phone changing its interval;
  - GATT reads and notifications work, and Gadgetbridge can connect;
  - the link drops cleanly when the phone leaves or goes out of range;
  - which nRF52832 revision the watch has (FICR at `0x10000134`), since the radio errata
    workaround depends on it.

## Bluetooth controller

See [bluetooth.md](bluetooth.md) for the details of each.

- [ ] **Encryption and pairing.** Answer `LL_ENC_REQ` and encrypt packets with the chip's CCM and
  ECB peripherals, so phones that insist on pairing can connect.
- [ ] **Sleep between connection events.** Wake from an RTC compare shortly before each event and
  start the 64 MHz crystal then, instead of keeping the crystal and TIMER0 running. This is the
  controller's biggest battery cost.
- [ ] **Peripheral latency.** Skip the connection events the central allows the watch to skip.
- [ ] **Faster transfers.** Data length extension, the 2 Mbit PHY and channel selection algorithm
  #2, mainly to speed up firmware updates.
- [ ] **Connection parameter request** from the peripheral. TrouBLE falls back to L2CAP meanwhile.
- [ ] **Skip the HCI byte layer** if flash gets tight. The controller could take TrouBLE's
  commands directly instead of encoding and decoding HCI bytes, which would save a few KB.

## Firmware updates

- [ ] **Update protocol over Bluetooth.** Not decided yet. The candidate is Nordic's legacy DFU
  service, as InfiniTime uses, so Gadgetbridge, ITD and Siglo can update NeoTime without changes.
  It would sit on the `services/updates` crate from the bootloader work.
- [ ] **Reloader image.** A firmware image, sent as a wasp-os or InfiniTime update, that moves the
  watch to NeoTime's bootloader. Stage NeoTime and the recovery firmware in external flash first,
  keep the window where a power cut bricks the watch as short as possible, refuse to run unless
  the watch is charging, and prove it in the Renode emulator (`PineTimeEmulator/`), power cuts
  included, before it touches a watch. Without SWD, a mistake here cannot be undone.
- [ ] **Feed the watchdog only while the button is up,** from the first firmware on. It is the only
  way back to the bootloader without SWD.

## UI toolkit

- [ ] **Measure the UI heap.** The memory map gives `rlvgl` a 14 KB heap, but that is a guess. Count
  what a typical screen allocates, then update the "UI heap" row in
  [memory-map.md](memory-map.md) and the size in `tools/draw_memory_maps.py`, and rerun the script.
- [ ] **Text edges over colour.** The display cannot be read back, so the soft edges of letters are
  mixed with one set background colour. Text on a coloured button gets dark fringes. A small line
  buffer would let edges mix with what is really underneath.
- [ ] **Page slides in the UI.** The simulator's demo pages already slide in with the hardware
  scroll. Move that into `neotime-ui` so rlvgl screens slide the same way.
- [ ] **rlvgl screens in the simulator.** The simulator still shows hand-drawn demo pages. Draw
  real rlvgl screens there through `ScreenRenderer`.
- [ ] **Touch input.** Turn `neotime-touch` gestures into rlvgl events.

## App runtime

- [ ] **Profiles.** Sets of apps such as work, gaming or comms, picked from the companion app. A
  profile is a small file in external flash listing app ids, their order, the watch face and which
  subscriptions are on. Switching profiles only reads files, so it writes nothing to flash.
- [ ] **Temporary apps from the phone.** The companion sends an app image straight into the app's
  RAM, such as a call screen or a game score display, and it exists only while the phone is
  connected. Nothing is installed, so nothing is written to flash.
- [ ] **Phone data as inputs.** Values the phone sends, such as the next calendar event, unread
  count, music track or a game score, become inputs like steps, so many apps are only a layout
  bound to phone data. Settles the open question in [app-runtime.md](app-runtime.md) about how
  phone data reaches the runtime.
- [ ] **Accelerometer configuration out of the firmware.** Bosch's configuration takes 6 KB of
  internal flash for each chip, 12 KB with both. Store it on external flash and send it to the
  sensor in small chunks at boot, turning step counting off if the file is missing. Other options:
  build in only one chip, or keep it in the shared library.
- [ ] **Internal flash writes while Bluetooth runs.** Erasing an internal flash page stops the CPU
  for tens of milliseconds, so the radio can miss its connection events. Updating the shared
  library or filling the internal-flash cache while the watch runs needs erases timed between
  connection events, or Bluetooth paused for the write.
- [ ] **Key-value store for app state.** Each app's state and settings are small values that
  change often. A log-structured key-value store, such as Zephyr's NVS or ZMS, Nordic's FDS or
  FlashDB, appends each change instead of rewriting a file, which suits that better than whole
  files in littlefs. FlashDB also has a time-series mode that fits step and heart-rate logs.

## Gaps against InfiniTime

What InfiniTime has for each feature NeoTime already has, compared with InfiniTime `6c119eb5`
on 2026-10-11. Items already listed in the sections above are not repeated.
The full report is [NeoTime vs InfiniTime: feature coverage](https://claude.ai/code/artifact/4a70bc3f-2b66-4bdf-8ba9-c9233c2f28ea).

### Display

- [ ] **Backlight.** Drive the three backlight pins with InfiniTime's levels: off, low, medium,
  high and always on.
- [ ] **Low-power mode.** The panel's 8-colour idle mode at a reduced frame rate, for an always-on
  display, as InfiniTime's `LowPowerOn` and `LowPowerOff` do.
- [ ] **Release the panel in sleep,** so the SPI bus can be turned off.
- [ ] **DMA transfers,** so the CPU is free while a buffer is sent. EasyDMA reuses the buffer, so
  wait for the transfer to end before writing it again.

### Touch panel

- [ ] **Wake on tap.** Let a single tap or a double tap wake the watch, chosen in settings.
  `sleep_until_double_tap` is the driver half.

### Vibrator

- [ ] **Callers.** Buzz for notifications, alarms, timers and hourly chimes.

### Side button

- [ ] **Actions.** Bind each gesture: wake, back, open notifications, go to the watch face.

### Accelerometer

- [ ] **Raise to wake and lower to sleep,** in software on `read_acceleration`.
- [ ] **Shake to wake,** with an adjustable threshold.
- [ ] **Step history:** today, yesterday, a resettable trip count, and the rollover at midnight.
- [ ] **Daily step goal** and a steps app.
- [ ] **Motion service over BLE,** sending steps and raw x, y and z to the phone.

### Bluetooth

- [ ] **Pairing with a passkey.** Secure Connections with a 6-digit passkey shown on screen, as
  InfiniTime does. Needs encryption in the controller first.
- [ ] **Bonding** saved to the file system and restored at start.
- [ ] **Privacy:** resolvable private addresses.
- [ ] **A BLE host in the firmware.** TrouBLE runs only in the tests so far.
- [ ] **A setting to turn the radio off.**

### UI toolkit

- [ ] **Double line buffers with DMA,** so one is filled while the other is sent. InfiniTime uses
  two of 240 x 4 pixels. This also fixes the text edges listed above.
- [ ] **Fonts and pictures** built into the firmware, and more loaded from the external flash.
- [ ] **A theme** and shared styles.

### Simulator

- [ ] **Simulated parts** for touch, the accelerometer, the battery and Bluetooth, plus
  notifications and heart rate once those exist.
- [ ] **GIF capture,** as InfiniSim has.

### External flash and storage

These live on the `worktree-bootloader`, `worktree-flash-async` and `worktree-updates` branches.

- [ ] **Deep power-down in sleep.** Put the flash chip to sleep when the watch sleeps.
- [ ] **Use the file system:** save settings, alarms and the BLE bond there.
- [ ] **File transfer over BLE,** with the Adafruit protocol InfiniTime's `FSService` uses, so
  companion apps can read, write and delete files.
- [ ] **Resource packages:** fonts and pictures installed from a zip over BLE.

### Firmware updates

- [ ] **An image checksum.** InfiniTime's DFU init packet carries a CRC; NeoTime checks only the
  vector table.
- [ ] **A progress screen** while an update is received.
- [ ] **Confirm or reject a new firmware** from settings, as InfiniTime's firmware screen does.
- [ ] **A bootloader screen** with a progress bar during a swap, and a sign that a held button
  will revert or recover. The screen stays dark for up to 30 seconds now.
- [ ] **A setting to turn updates off,** or on until the next reset.
- [ ] **The recovery firmware** itself.
- [ ] **Show the bootloader version** in a system info screen.

### Not started

InfiniTime has these and NeoTime has nothing for them yet:

| Area | What InfiniTime has |
|---|---|
| Board and system loop | pin setup, tasks, sleep and wake, wake locks, the watchdog, a system monitor |
| Heart rate | HRS3300 driver, PPG processing, background measuring, the Heart Rate service |
| Battery | voltage and percentage, charging and power-present pins, the Battery service |
| Clock | RTC date and time, 12 or 24 hours, the Current Time service and client |
| Settings | watch face, wake-up mode, chimes, screen timeout, step goal, weather units |
| Notifications | Alert Notification service and client, Immediate Alert, notification list, calls |
| Other BLE services | Device Information, Music, Navigation, Simple Weather |
| Watch faces | Digital, Analog, PineTimeStyle, Terminal, Infineat, Casio G7710, Pride Flag |
| Apps | Alarm, Timer, Stopwatch, Music, Navigation, Weather, Calculator, Metronome, Flashlight, Paint, Paddle, 2048, Dice, System Info |

## Gaps against the wasp-os fork

What the BeatLink wasp-os fork has for each feature NeoTime already has, compared with its
`development` branch at `4f065f7` on 2026-10-11. Items already listed above are not repeated. The
full report is
[NeoTime vs wasp-os: feature coverage](https://claude.ai/code/artifact/1676468a-1a4f-421b-ac54-345934b08570).

### Display

- [ ] **Mute while switching apps,** so a half-drawn screen is never shown.
- [ ] **Measure the bus speed on the watch.** The fork found its SPI bus stuck at 1 Mbps instead of
  8 Mbps, which made every redraw 4.4 times slower.

### Touch panel

- [ ] **Pass touch and swipe events to apps.**

### Side button

- [ ] **Click goes home,** and a second click on the home screen puts the watch to sleep, as the
  fork does.

### Accelerometer

- [ ] **Write the axis remap to the chip,** so its own motion features see the watch the right way
  up. The driver only remaps in software now.
- [ ] **Log steps to the file system** every 6 minutes, one file a day, with no limit on history.
- [ ] **Reset at midnight** without the steps app open.
- [ ] **Per-day step graphs** in the steps app.
- [ ] **Write raise to wake independently.** The fork's detector copies InfiniTime's GPL-3.0 rules.

### Bluetooth

- [ ] **Nordic UART Service,** the transport the fork's companion apps use.
- [ ] **The Gadgetbridge Bangle.js protocol:** notifications, dismissals, music, weather, find
  phone, time, alarms, steps, settings, calls and HTTP, plus watch status reports. Decide whether
  NeoTime speaks this, InfiniTime's GATT services, or both.
- [ ] **A developer console over BLE,** to replace the fork's Python REPL for uploads, time setting
  and resets.

### UI toolkit

- [ ] **Cards:** rounded-rectangle cards as the layout every app uses.
- [ ] **Widgets:** status bar, battery meter, clock, notification bar, scroll indicator, button,
  toggle, checkbox, slider, spinner, stopwatch and confirmation view.
- [ ] **App icons:** monochrome Font Awesome icons per app.
- [ ] **Theme presets** set from the companion app.
- [ ] **Grid and list launchers,** chosen in settings.

### Simulator

- [ ] **Run every app and face** in the simulator.
- [ ] **A smoke test** that opens every app in the simulator and checks its screenshot.

### External flash and storage

- [ ] **Install apps as packages over BLE,** as the fork's `pkgmgr` does into `/flash/pkg`.
- [ ] **File manager and backup** from the companion app.
- [ ] **Updatable read-only image:** the fork keeps nearly all its code in a ROMFS image in
  internal flash, updated without reflashing the firmware.

### Firmware updates

- [ ] **Keep the clock across a reset** in retained RAM.
- [ ] **Firmware from GitHub releases** offered by the companion app.

### Not started

The fork has these and NeoTime has nothing for them yet. Most also appear in the InfiniTime table.

| Area | What the fork has |
|---|---|
| App runtime | apps loaded when opened, state kept when left, a Software app to turn apps on and off, a crash screen |
| Heart rate | filter-chain PPG (30 to 210 BPM), raw data logging |
| Watch faces | Clock, Week clock, Chrono, Dual clock, Fibonacci, Resistor, Word clock, a Faces app with previews |
| Apps | Alarm with 4 alarms and snooze, Pomodoro, Phone finder, Sports, Level, Gallery, Haiku, Morse, Snake, Four in a row, Puzzle 15, Game of Life, Beacon, Self test |
| Companion app | WaspOSCompanion: steps, alarms, settings, files, backup, weather, themes, face previews, firmware |
| Developer tools | `wasptool`, `pkgpush.py`, `nus.py` |
| Other watches | Colmi P8, Senbono K9, DS-D6, 96Boards Nitrogen |

## Other firmware to study

Not compared yet. Look through each for features worth bringing to NeoTime.

| Firmware | What it is |
|---|---|
| Pinetime-Lite | An InfiniTime fork by Joaquimorg with many watch faces and extra features. |
| Hypnos | A C firmware on the Zephyr RTOS, with basic watch features and BLE time sync. |
| pinetime-rust-mynewt | Rust apps on Apache Mynewt, with many articles by Lup Yuen Lee. |
| ATCwatch | Arduino firmware by Aaron Christophel, first written for the Colmi P8. |
| Espruino | The JavaScript interpreter behind the Bangle.js watches, with its large app loader. |
| TinyGo | Supports the PineTime as a target for firmware written in Go. |
| RIOT OS | Has board support for the PineTime. |
| Klok | A Rust clock firmware on Apache Mynewt by Casper Meijn. |
| pinetime-rtic, pinetime-rtfm | Bare-metal Rust firmware and demos on RTIC, earlier called RTFM. |
| watchful | Rust firmware and drivers by Ulf Lilleengen. |
| timecake, Pyrus | Bare-metal firmware with no RTOS. |
| pinetime-os | A FreeRTOS firmware by kaythe. |
| pinetime-zephyr, Hermes, ztime | More Zephyr firmware, by najnesnaj, Dejvino and AlexXZero, plus SuperPrower's Zephyr sample. |
| mbed-pinetime | Mbed OS ports by sethitow and geoffrey-vl, with a set of PineTime drivers. |
| PineTime-apps | Apps on RIOT OS by bosmoment, with its own Bluetooth UUID and a Gadgetbridge fork. |
| go-smartwatch | A Go firmware by aykevl on TinyGo. |
| PineTimeSD | An InfiniTime fork that streams sensor data to OpenSeizureDetector. |
| p8b-infinitime | An InfiniTime fork with 200 Hz accelerometer streaming, and clock calibration for watches without the 32 kHz crystal. |
| Malila | Announced in 2021 as a new PineTime OS; no release found. |
