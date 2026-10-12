# To do

Work that is known to be needed but not done yet.

When an item is done, remove it here. If it is something the user sees, add it to
[features.md](features.md); if not, describe it in [architecture.md](architecture.md) or the
document for its part instead.

## Hardware tests

- [ ] **Touch panel on the PineTime.** The firmware runs `neotime-touch` on the watch, but these
  checks have not been made:
  - `start` succeeds and `read_chip_info` gives a sensible chip ID (0xb4 or a near value);
  - taps, long presses and slides in all four directions come through as the right `Gesture`;
  - a drag streams reports while the finger stays down, and the finger lifting is reported;
  - positions match the display, with column 0 and row 0 at the top left;
  - `sleep` stops the interrupts, and `sleep_until_double_tap` wakes on a double tap only;
  - `wake` brings back the settings `start` was given.

- [ ] **Accelerometer on the PineTime.** The firmware counts steps with
  `neotime-accelerometer` on the watch, but these checks have not been made:
  - `start` succeeds and names the chip the watch really has, which on this watch is a BMA425;
  - lying flat with the screen up reads about +1000 milli-g on z, and tilting it reads the
    expected signs on x and y, which settles whether `Orientation::PINETIME` is right;
  - walking raises `read_step_count`, and `reset_step_count` sets it back to 0;
  - the upload still works with chunks larger than 16 bytes, which would make starting faster.

- [ ] **Bluetooth controller on the PineTime.** The firmware runs `neotime-bluetooth` on the watch and
  takes updates over it, but these checks, with a phone and with `btmon` on the computer, have not
  been made:
  - advertisements show up on all three channels, with the right name and address;
  - a scan request gets the scan response;
  - a phone connects, and the connection survives the phone changing its interval;
  - GATT reads and notifications work, and Gadgetbridge can connect;
  - the link drops cleanly when the phone leaves or goes out of range;
  - which nRF52832 revision the watch has (FICR at `0x10000134`), since the radio errata
    workaround depends on it.

- [ ] **Settings on the PineTime.** The Settings app, the settings store and the companion's
  `settings` keys have run only in the simulator and the emulator. On the watch, check that:
  - each switch and choice in the Settings app takes effect: brightness at once, the screen
    timeout on the next idle, 12 or 24 hours and units on the face and apps;
  - the always-on screen dims at a steady, flicker-free level instead of turning off;
  - chimes buzz on the hour and half hour, and stay silent in do not disturb;
  - a preset theme recolours the open app and the next one opened;
  - settings survive a restart, from `/settings/system` and `/settings/theme`;
  - a `settings` message from the companion app changes them and the reply lists every key.
- [ ] **Files over BLEFS on the PineTime.** The file service has run only in the emulator, against a
  raw HCI client. On the watch, with the companion's `neotime files` commands, check that:
  - the phone reaches an MTU of 124, so a write of up to 121 bytes is taken whole;
  - listing, making a folder, writing a file, reading it back, moving it and deleting a folder work;
  - a long listing arrives complete, with no notification lost while packets are short;
  - free space reads back through the companion's scratch-file write.
- [ ] **Installing apps on the PineTime.** The packages characteristic has run only in the emulator
  and against the companion's mock watch. Once the firmware without built-in apps is on the watch,
  run the companion's setup flow and check that:
  - the shared library installs into internal flash and survives a restart;
  - the face, the launcher and the apps install from `/apps` and appear without a restart;
  - `apps` lists them, and hiding, showing and removing one updates the launcher;
  - an image with a bad id or a library that fails its check is refused with an error.
- [ ] **Reset reasons and crash record on the PineTime.** They have run only in the emulator. On the
  watch, check that:
  - the status page reads "Reset: power on" after the battery is reconnected, "Reset: watchdog"
    after a button hold, and "Reset: software" after an update;
  - when a panic or a HardFault happens, it shows on the next start's status page and in the
    diagnostics characteristic's last-restart words, and a panic's addresses lead to its caller;
  - an ordinary start after that shows the crash no more.

- [ ] **Transitions and scrolling on the PineTime.** Slides, wipes and hardware-scrolled lists have
  run only in the emulator and against a model of the panel; none of it is on the watch yet. Flash
  the bootloader first, then a firmware built from master at e78fc7a or later. On the watch, check
  that:
  - a slide and a wipe each take about a quarter of a second, and a slide shows no tearing at its
    leading edge;
  - the launcher and the Settings list follow a finger without lag, and reach their last row;
  - opening Settings from a scrolled launcher, where the outgoing launcher is shown out of place
    while it slides away, looks acceptable, or whether that unturning should move elsewhere;
  - the stack keeps 2 KB spare with the alarm open while its list scrolls.

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

See [updates.md](updates.md). The reloader moved a watch from wasp-os onto NeoTime, and an update
over Bluetooth and the button-hold recovery have both worked on that watch.

- [ ] **Check why wasp-os's update mode gives way to the reloader** about 4 seconds after the
  button escape in the emulator, with no reset between. Its advertising never times out, so this may
  be the emulator's SoftDevice; if a watch does the same, the escape leaves the reloader running.
- [ ] **Run the DFU service against more clients:** wasp-os's `dfu.py` works on the watch; the
  companion app and Gadgetbridge are next.
- [ ] **Async flash in the firmware.** Flash writes block the Bluetooth host for up to a sector
  erase at a time, because the display shares the SPI bus and its driver is blocking.
- [ ] **Signed updates**, through embassy-boot's ed25519 check in `UpdateReceiver::finish`.
- [ ] **Run the emulator suites in CI** once the faster emulator lands. CI runs none of
  `tests/emulator/` today, so the bootloader menu's tests in `update_and_recovery.robot` (rollback,
  recovery, the timeout) have only run by hand, and on Renode they take well over an hour.
- [ ] **Read the bootloader in the updater with assembly loads.** `bootloader-updater`'s
  `installed_crc` reads `BOOTLOADER.start` (address 0, the null pointer) through `Nvmc`, which is
  undefined behaviour; the firmware's same read let the compiler delete most of the firmware. Use
  loads like `status::BootloaderPages` in the firmware.
- [ ] **A Gadgetbridge entry for NeoTime.** Gadgetbridge recognises InfiniTime by its advertised
  name; NeoTime advertises as "NeoTime" and needs a device coordinator of its own, or to be
  matched by the PineTime one.

## UI toolkit

- [ ] **Measure the UI heap on the watch.** It is 21 KB, shared with the 16 KB storage heap; so
  far it is measured only in the emulator, where the alarm peaks at 36,188 B of 37,888 B. Count what a typical screen allocates on
  the watch, then update the "UI heap" row in
  [memory-map.md](memory-map.md) and the size in `tools/draw_memory_maps.py`, and rerun the script.
- [ ] **Text edges over colour.** The display cannot be read back, so the soft edges of letters are
  mixed with one set background colour. Text on a coloured button gets dark fringes. A small line
  buffer would let edges mix with what is really underneath.
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
- [ ] **Resource cache for external flash.** A small fixed area of RAM with two or three slots,
  reused least recently used first. Each slot is filled by one DMA read of a whole resource, such
  as a font or picture from a resource package or a handler from `/apps`, so each costs one SPI
  transaction instead of many small reads.
- [ ] **The remaining companion messages.** [companion-protocol.md](companion-protocol.md) lists
  them under "not served yet". `call`, `musicinfo`, `musicstate`, `weather`, `find`, `vibrate`,
  `status` and the `music`, `call` and `findPhone` lines are served (`services/companion/src/phone.rs`).
  - `alarm` (set or report the alarms), `steps` (a day in 240 six-minute slots, which needs a step
    history kept on the watch) and `http` (answers to an app's request, and the watch's `http`
    line).
  - `request_music_state` and `request_navigation_state` send nothing, since Bangle.js has no such
    message; find what Gadgetbridge answers, or add one for the companion app.
  - Navigation: Gadgetbridge's newer Bangle.js driver sends `nav`; read it into
    `current_navigation` and run the `navigation` subscriptions.
  - The weather's coming days, sunrise and sunset: Gadgetbridge's `weather` line carries none, so
    the forecast stays empty until the companion app sends them in keys of its own.
  - Keep the music and weather through a restart, as [app-reference.md](app-reference.md) promises;
    they are kept in RAM only.
  - A temperature with a fraction makes the `weather` line unreadable, since the watch parses
    whole numbers only to stay clear of float parsing's flash; round it if a phone sends one.
  - Nothing has run on the watch yet: try each message from Gadgetbridge and the companion app.
- [ ] **Harness gaps in the app host.** The test harness works around three: `widget_bounds` has
  no clip rectangle, so it cannot tell a widget scrolled out of a List or Scroll; there is no
  iterator over the open screen's nodes; and there is no public "redraw everything" (it re-applies
  `set_theme`).
- [ ] **Read handlers from the file when they run.** The shell reads an app's whole image into the
  UI heap when it opens, so every byte of it costs RAM. Keep only the expressions and the functions
  they call, which run on every redraw, in RAM and read a handler from `/apps` when its event
  comes, as [app-runtime.md](app-runtime.md) plans. Apps stay on the external flash; nothing of an
  app is copied into the internal flash.
- [ ] **Resize the firmware slot and the shared library.** With no app, face or font built in the
  firmware is 289 KB, so a split such as 320 KB firmware and 172 KB library becomes possible if the
  library (74,876 B of 92 KB) fills up. The sizes are in `neotime-boot`, which the bootloader shares,
  so do it together with the next bootloader update that is needed anyway.

## Shared library and smaller apps

The 49 apps and faces hold 105 KB of images, down from 197 KB now that the compiler calls an app's
own functions, shares code that repeats with other constants and names library icons (see
[compiler.md](compiler.md#handlers)). Code that is byte-for-byte the same across apps is only
about 4 KB, so sharing between apps pays only as library functions that take arguments. Library
code is read in place from the internal flash and costs no RAM. The items run in order; each needs
the ones above it.

- [ ] **Calls into the shared library.** `call_library` runs a function from a functions section
  in the library image, checked by the same verifier, with numbers that never change between
  library versions. A function that changes state takes the variables by number, as `copy array`
  names its arrays. One-line helpers such as `(x + 1) % n` stay inlined, since a call costs as
  many bytes.
- [ ] **Repeated rows outside lists.** A `.map` inside a `List` or `Scroll` is one `Repeat`; in
  a `Column`, `Row` or `Grid` it is still unrolled, as in the settings choices, the weather
  forecast and the alarm's weekday buttons. Laying a Repeat's rows out as its parent's own
  children, so `justify` and grid columns treat them as before, would let those share one row too.
- [ ] **Native widgets with their logic built in.** Every widget node costs 160 to 240 B of the UI
  heap wherever its record lives, so a composite drawn by the firmware as one node saves more RAM
  than a library component. In the firmware, which has 112 KB free:
  - `AppGrid`, the installed apps as a grid of icons, beside the `AppList` the launcher uses.
  - `StatusIcons`: battery, Bluetooth, notifications and alarm, with a style for the wasp-os bar.
    Replaces 13 `BatteryIcon` copies and 9 hand-made status rows in the faces.
  - `TimePicker`, `DurationPicker`, `WeekdayPicker` and `Stepper`, writing back to a signal, for
    the alarm, timer, pomodoro, settings and metronome.
  - A `confirm(question, handler)` effect drawn over the screen, in place of `ConfirmDialog`,
    whose 6 nodes stay allocated while hidden (infinipaint, morse, puzzle15, twos).
  - A `ring(title, icon, until)` system alert with dismiss and snooze events, with the vibration
    and the stop after a time limit, for the alarm, timer and pomodoro.
  - Numbers drawn in `CellGrid` cells, so twos and puzzle15 need 1 widget instead of 16 labels.
  - A digit-entry mode for `ButtonMatrix`, for the calculator and pomodoro.
  - `TextLines` over an array of character codes, for morse's 120 lines of wrapping.
  - `SystemBadge` for steps, heart rate, weather and battery text, used in about 8 faces.
- [ ] **Native array builtins.** Whole-board work is smaller and far faster in Rust than in
  bytecode loops: `slide_board` and `board_can_move` (twos), `line_length_through`
  (four-in-a-row), `life_step` (game-of-life, about 2,000 steps a generation now),
  `random_cell_where`, `count_equal`, `shuffle`, `permutation_parity`, `arrays_equal`,
  `push_shift` (history lists in heart, motion, steps, stopwatch and morse), `format_fixed`
  (calculator), `format_distance` (navigation, which ignores the units setting), `digit_of`,
  `sunday_week_of` and `days_in_year`.
- [ ] **Library functions.** Logic with a real body that two or more apps share:
  - Time: `elapsed_clock` (start, pause, reset; stopwatch, sports, metronome, heart), `countdown`
    and `remaining_seconds` (timer, pomodoro), `dial_hour` (10 faces and the alarm),
    `day_mask_name` and `format_time_of_day` (alarm).
  - Weather: `is_night`, `weather_icon_now`, `has_weather`, `weather_condition_name`,
    `temperature_in_unit` and `temperature_color` (weather, digital, pinetime-style, terminal,
    lite-weather).
  - Steps: `steps_since`, which survives the midnight reset (steps, sports), and
    `step_goal_percent`.
  - Games: `record_best`, `step_for(direction)`, `neighbour_in`, `bounce_axis` and `move_toward`
    (snake, paddle, twos, puzzle15, four-in-a-row, lite-pong).
  - Editing values: `step_clamped`, `step_wrapped`, `append_digit` and `drop_digit` (settings,
    metronome, pomodoro, calculator).
- [ ] **Library components.** Widget trees with props, for layouts that need no native logic:
  `ActionRow` and `PlayPauseButton` (7 apps), `ToggleScreen` (phone-finder, disable-bluetooth,
  flashlight, beacon), `SettingSwitch` and `ChoiceList` (settings), `CycleChooser` and
  `DoneButton` (infineat, pride-flag, pinetime-style), `GameOverPanel`, `ScoreHeader`,
  `EmptyState`, and `ClassicDial` with `DateWindow` (analog, lite-analog).
- [ ] **Library strings and palettes.** Start, Stop, Reset, Cancel, Dismiss, Snooze, Once, Every
  day, Weekdays, Weekends, Game over; InfiniTime's 18 colours, the resistor code and a rainbow; a
  theme colour for the `#b0b0b0` that 9 apps write out.
- [ ] **Fixes found on the way.** The weather app rounds Fahrenheit differently from
  `format_temperature`, and pinetime-style has a third version. Several apps rebuild `Header`,
  `Card`, `clamp`, `indexOf` or `count_where` by hand. `StatusBar`, `ListRow` and `Menu` are used
  by nothing.

## RAM

- [ ] **Stack margin with the alarm open.** Measured in the emulator with a typical install, the
  alarm peaks at 10,516 B of 12,436 B, 1,920 B spare, under the 2 KB [memory-map.md](memory-map.md)
  asks for. Find what makes that path deep, or win static RAM back, before anything grows.

- [ ] **Shrink the storage heap.** It is 16 KB, but littlefs needs about 1.8 KB with two files
  open, and the rehearsal build runs on 4 KB. Measure the peak on the watch, including file
  transfer over Bluetooth, then cut it down; up to 12 KB could be freed.
- [ ] **Smaller Bluetooth buffers.** Fewer and smaller packets in the pool, one connection and a
  trimmed GATT table take RAM from the 12 KB budget, at some cost to file transfer speed. The
  Bluetooth task's state is 4.8 KB, most of it TrouBLE's GATT table (39 attributes of 44 B, 2.1
  KB) and its runner (1.6 KB); shrinking those means patching `third_party/trouble-host`.
- [ ] **Trim the stack.** 13.0 KB is left for it; in the emulator it peaks at 8.3 KB with most apps
  and 10.4 KB with the alarm open. Measure the high-water mark on the watch before giving any of it
  away.
- [ ] **Find large static RAM.** List `.bss` and `.data` by size with `nm --size-sort` and check
  for oversized embassy task state or tables that should be `const`.

## Apps and faces

Found by the harness's snapshots and quality checks; see [testing.md](testing.md).

- [ ] **Calculator's 7 key is always highlighted,** because the compiler cannot pass labels to
  `highlighted` (see [compiler.md](compiler.md)).
- [ ] **Terminal face overflow.** Its last line falls off the bottom of the screen and
  "[STAT] Connected" is cut in half.
- [ ] **Apps over the RAM budget.** The quality check's `KNOWN_GAPS` lists alarm (67 widgets on
  its main screen) and the launcher (65) against a widget budget worked out from 240 B nodes. Nodes
  are 160 B now and the alarm opens in the emulator, at 36,188 B of the 37,888 B heap, so redo
  the budget in the check and [memory-map.md](memory-map.md). The alarm's image has since shrunk
  from 15 KB to 6.1 KB, so measure it again. The launcher runs on the watch, so measure the real limit and either correct
  [memory-map.md](memory-map.md) or slim the screens.

## Display and SPI bus

- [ ] **Share the SPI bus between the screen and the flash.** The XT25F32B is on the same bus as
  the screen, and the runtime reads handlers and images from external flash while screens are
  drawn. Both drivers need a shared-bus wrapper, such as the shared SPI in
  `embassy-embedded-hal`, so neither can cut into the other's transfer.
- [ ] **Draw one band while sending the last.** After DMA transfers work, use two band buffers,
  such as 240 by 16 pixels at 7.5 KB each, so drawing the next band hides behind sending the
  previous one.
- [ ] **12-bit colour mode.** The ST7789 can take 12 bits a pixel, which cuts the time to send a
  full screen by a quarter, from about 115 ms to 86 ms at 8 MHz. It costs banding in gradients and
  packing pixels in pairs. Measure it once real screens run on the watch.

## Gaps against InfiniTime

What InfiniTime has for each feature NeoTime already has, compared file by file with InfiniTime
`6c119eb5` on 2026-10-11. Items already listed in the sections above are not repeated. On the
`worktree-prune-superseded` branch of the BeatLink InfiniTime fork, the files NeoTime has fully
replaced are removed, so what is left there is the porting reference for these items.
The full report is [InfiniTime files against NeoTime](https://claude.ai/artifact/LqGLNFf5MvT5D39o44hfUv); the earlier feature summary is
[NeoTime vs InfiniTime: feature coverage](https://claude.ai/code/artifact/4a70bc3f-2b66-4bdf-8ba9-c9233c2f28ea).

### Board and system

- [ ] **Turn on the DC/DC regulator.** The firmware keeps embassy-nrf's default, the LDO.
  InfiniTime sets `DCDCEN` at start, which cuts the current the chip draws, most of all while the
  radio is on. Set `config.dcdc.reg1 = true` in the firmware and the bootloader.
- [ ] **Recover the I2C bus.** InfiniTime clocks SCL 16 times at start to free a bus a sensor left
  held, runs the TWIM at about 390 kHz because of the nRF52832's 400 kHz timing erratum, and resets
  the TWIM when a transfer never finishes. NeoTime runs at 400 kHz with no time limit, so a frozen
  bus stops touch and steps for good.
- [ ] **Warn when a part fails at start.** InfiniTime shows a warning screen when the touch
  controller does not start; NeoTime shows nothing.
- [ ] **A system info screen** showing the firmware version, which the status page and the Device
  Information service already carry. Apps read only system values, so it needs a text value such
  as `firmware_version` through the runtime, the compiler, the SDK and the preview.

### Display

- [ ] **Try the always-on screen on the watch.** Turn on Always on in the Settings app and
  check that the dim level is steady, without flicker, that the face keeps time, and how much
  battery a night costs. It has only been built, not run.
- [ ] **Dim before the screen turns off.** Drop to the low backlight a few seconds before the
  timeout, so a wearer still reading can touch to keep it on, as InfiniTime does.
- [ ] **The panel's power settings.** mipidsi starts the ST7789 with its defaults. InfiniTime also
  sets the lowest panel and gate voltages, a VDV value that makes the first grey above black less
  of a jump, and its own porches and frame rate. Send the same commands after `Display::new`, then
  compare the current drawn and the dark greys on the watch.
- [ ] **Release the panel in sleep,** so the SPI bus can be turned off.
- [ ] **DMA transfers,** so the CPU is free while a buffer is sent. EasyDMA reuses the buffer, so
  wait for the transfer to end before writing it again.

### Touch panel

- [ ] **Wake on tap.** Let a single tap or a double tap wake the watch, chosen in settings.
  `sleep_until_double_tap` is the driver half.
- [ ] **Sleep the touch panel with the screen.** The firmware leaves the panel scanning while the
  screen is off. Call `sleep` when the screen turns off, and `wake` when it turns on.
- [ ] **Double taps on the watch.** The firmware starts the panel with double taps off, so
  `onDoubleClick` never fires on the watch. Turn them on when the open screen or a wake setting
  wants them.

### Vibrator

- [ ] **Let the wearer silence alarms in do not disturb.** Alarms and timers ring through do not
  disturb, which app-reference.md says the user can turn off. Add the setting, and keep app
  permissions in the image so the watch can check `wake` instead of trusting the `alarm` pattern.
- [ ] **Try the buzzes on the watch:** a phone notification buzzes twice and stays silent in do
  not disturb, and the alarm pattern still rings there. Check too that a notification on a dark
  screen turns it on into the notifications app, that a `notify-` from Gadgetbridge removes it, and
  that the heap holds with five kept beside the alarm app.

### Side button

- [ ] **Actions.** Bind each gesture: wake, back, open notifications, go to the watch face.
- [ ] **Use the button driver in the firmware.** The firmware polls the pin every 20 ms with its
  own long press, so `neotime-button`'s double click and 2 second press never reach apps, and the
  poll runs while the watch sleeps. Wake on the pin's edge and send the driver's gestures instead,
  keeping the watchdog fed only while the button is up.

### Accelerometer

- [ ] **Raise to wake and lower to sleep,** in software on `read_acceleration`.
- [ ] **Shake to wake,** with an adjustable threshold.
- [ ] **Step history:** today, yesterday, a resettable trip count, and the rollover at midnight.
- [ ] **Daily step goal** and a steps app.
- [ ] **Motion service over BLE,** sending steps and raw x, y and z to the phone.

### Clock

- [ ] **Set the date and time on the watch,** with rollers for the date and the time (with AM/PM on
  a 12 hour clock), for when no phone has set the clock.

### Settings

- [ ] **A settings app** with Display, Wake up, Time format, Units, Watch face, Steps, Date and
  time, Chimes, Shake calibration, Firmware, Updates, Bluetooth, Heart rate and About. The firmware
  must carry out `set_setting`, save each change to `/settings` and read them back at start, so the
  24 hour clock, units, step goal, Bluetooth and screen timeout stop being fixed in
  `firmware/src/shell.rs`.

### Bluetooth

- [ ] **Pairing with a passkey.** Secure Connections with a 6-digit passkey shown on screen, as
  InfiniTime does. Needs encryption in the controller first.
- [ ] **Bonding** saved to the file system and restored at start.
- [ ] **Privacy:** resolvable private addresses.
- [ ] **A setting to turn the radio off.**
- [ ] **Advertise like InfiniTime.** The firmware advertises every 160 ms all the time. InfiniTime
  advertises fast for 30 seconds after start or a disconnect and then about once a second, puts the
  heart rate and DFU service UUIDs and the transmit power in the advertisement, and the name in the
  scan response, which some phone apps filter on.
- [ ] **A larger ATT MTU.** The firmware allows 64 bytes where InfiniTime asks for 256. File
  transfer, long track names and notifications need longer writes.

### App runtime

- [ ] **Run closed apps on the watch.** The shell runs only the open app and never calls
  `open_in_background`, so an alarm or timer rings only while its app is open, and the steps app
  misses midnight while closed. Run each installed app's background subscriptions, set
  `next_alarm` from the earliest alarm so faces show the bell, and carry out `wake_screen`.
- [ ] **Deliver the remaining sensor and phone events to apps.** The shell pushes steps,
  acceleration, shakes and phone notifications. Push the other `motion` kinds (wrist raise, tap,
  double tap, still, moving), heart rate, battery changes, calls, music, navigation and weather
  once those services exist.
- [ ] **Try acceleration and shakes on the watch.** The motion and level apps should follow the
  watch's tilt with the right axes and signs, a firm shake should roll the dice once, and walking
  should not. Tune `JOLT_THRESHOLD_IN_MILLI_G` if needed.

### UI toolkit

- [ ] **Double line buffers with DMA,** so one is filled while the other is sent. InfiniTime uses
  two of 240 x 4 pixels. This also fixes the text edges listed above.
- [ ] **More fonts and pictures** loaded from the external flash, beyond the shared library's.
- [ ] **Drag to change an Arc.** The app host gives `Arc` no `onChange`, so the metronome's tempo
  ring cannot be dragged; only tap tempo works.
- [ ] **A canvas widget for free drawing.** Paint fills cells of a 20 by 20 grid; InfiniTime
  paints where the finger is.
- [ ] **Pages and Tabs widgets** that swipe between pages with a page indicator, as InfiniTime's
  paged lists do. Both are laid out as a plain column now.
- [ ] **Cyrillic text.** InfiniTime's 20 px font has Cyrillic letters for notifications and music
  titles. Add them to the text fonts in `tools/generate_fonts.py`.
- [ ] **More library icons:** brightness levels, sleep, notifications on and off, info, list,
  home, eye, ban, phone off, volume mute, tachometer, paintbrush, paddle, map, shield, and the
  moon and night rain weather icons. Quick settings and the ports need them.
- [ ] **UI design guidelines.** Add InfiniTime's rules to `app-reference.md`: buttons at least 50
  pixels high and on the bottom edge, room for a page indicator and top bar, switches twice as
  wide as tall, and black backgrounds.

### Apps and faces

- [ ] **The full set of navigation turns.** Phones send about 50 icon names, such as forks,
  merges, ramps, ferries and roundabout exits, and the distance as text. NeoTime has 10 turns and
  always prints metres. Add InfiniTime's navigation pictures, map each name to one, and show the
  distance as the phone writes it.
- [ ] **Night icons in the weather app.** It shows the sun at night where InfiniTime shows the
  moon. One shared night-aware lookup would also replace the copies in the faces.
- [ ] **A 40 pixel seven-segment font,** so the Casio face's day-of-year and date lines match
  InfiniTime.
- [ ] **Bebas Neue and Teko for Infineat,** which draws its time and date in them; NeoTime
  substitutes Open Sans and JetBrains Mono.
- [ ] **Music disc artwork.** InfiniTime's music app shows a disc that spins while a track plays.

### Simulator

- [ ] **Simulated parts** for touch, the accelerometer, the battery and Bluetooth, plus
  notifications and heart rate once those exist.
- [ ] **GIF capture,** as InfiniSim has.

### External flash and storage

These live on the `worktree-bootloader`, `worktree-flash-async` and `worktree-updates` branches.

- [ ] **Deep power-down in sleep.** Put the flash chip to sleep when the watch sleeps.
- [ ] **Use the file system:** save alarms and the BLE bond there.
- [ ] **Resource packages:** fonts and pictures installed from a zip over BLE.
- [ ] **Report failed flash writes.** After each erase and page program, read the flash's security
  register and return an error when its erase-fail or program-fail bit is set, so littlefs moves
  the block instead of trusting it.

### Firmware updates

- [ ] **Escaping the reloader under InfiniTime leads back to the reloader.** InfiniTime's DFU
  installs the reloader as an unconfirmed MCUBoot test image, and the reloader stages NeoTime over
  MCUBoot's secondary slot (SPI 0x40000 to 0xB4000), where the previous InfiniTime waits. After
  that, a reset does not bring InfiniTime back. MCUBoot 1.5.0, which bootloader 1.0.0 is built on,
  checks the slot before reverting, finds no valid image, erases it and starts the reloader again,
  and the bootloader's button-hold rollback and factory restore end the same way. So no reset
  bricks the watch, but a reloader that keeps failing, such as on a bad flash chip, cannot be left
  without SWD. The comment on `feed_watchdog_while_button_is_up` in `reloader/src/main.rs`, which
  says a held button returns to the old update mode, is only true for wasp-os. Decide whether the
  reloader should keep InfiniTime's slot until its own checks pass, and write InfiniTime's flash
  layout into [updates.md](updates.md). Checked from source only, not in the emulator.
- [ ] **Test the InfiniTime migration against the bootloader watches ship with.**
  `migration.robot` loads lupyuen's bootloader 5.0.4 from the InfiniTime checkout and puts the
  reloader straight into its slot. Watches carry JF002's bootloader 1.0.0 and get the reloader
  through InfiniTime's DFU, which swaps it in from SPI 0x40000. Fetch both bootloaders in the
  flake and drive the real swap.
- [ ] **Confirm or reject a new firmware** from settings, as InfiniTime's firmware screen does.
- [ ] **Update progress as a bar.** The status page shows the kilobytes, the image size and a
  percentage in words. Draw a bar under them as well, as InfiniTime does.
- [ ] **Give up on a stalled update.** InfiniTime aborts an update when no packet has arrived for
  10 seconds; NeoTime only aborts when the link drops.
- [ ] **A swap progress bar on the bootloader screen.** It says "Installing firmware" for up to 30
  seconds with nothing moving. Its 16 KB has about 2 KB to spare.
- [ ] **A setting to turn updates off,** or on until the next reset.
- [ ] **Show the bootloader version** in a system info screen.

### Developer tools and docs

- [ ] **A DFU upload tool in `tools/`** that sends a zip over BlueZ, as wasp-os's `dfu.py` does,
  including type `0x08` for the recovery slot, which no client sends yet.
- [ ] **Flashing a watch over SWD.** One image with the bootloader and the firmware, a way to fill
  the recovery slot on a watch flashed this way, and a page on probes, the SWD pins and RTT logs.
- [ ] **Releases.** A version scheme, tagged releases carrying `dist/`, and a page saying what
  each file is for.
- [ ] **How to measure memory.** Say in [memory-map.md](memory-map.md) how each figure is measured
  on the watch: flash by crate and symbol, the stack's high-water mark, and heap peaks.
- [ ] **Run the Renode suites in CI.** `tests/emulator/*.robot` are the only tests of the
  firmware, bootloader and reloaders, which build only for the watch, but CI does not run them.
  Package the patched Renode from PineTimeEmulator for the workflow and add a job.
- [ ] **Firmware logic where tests reach it.** Coverage measures only the workspace. Move logic out
  of `firmware/` into workspace crates, as `boot` and the services already are, so host tests
  cover it.
- [ ] **Harness gaps.** Open a named screen directly, so screens behind icon-only buttons, such as
  the alarm's edit and ring screens, get smoke and snapshot coverage; read the key labels of a
  `ButtonMatrix`, so `tap "7"` works on the calculator; match text spread over neighbouring
  labels, such as a face's "09:00"; set text system values from `TestWatch`; give `notify` an
  app name in scripts; and see every built row of a repeated list, since `widget_text` and
  `widget_bounds` give only the first row's node, so `tap "Sam"` cannot reach the second row of
  the notifications list.
- [ ] **Scripts from an installed simulator.** `--script` finds app sources through the build-time
  `CARGO_MANIFEST_DIR`, so it works only through `cargo run` in the repository, not from
  `nix run`.
- [ ] **App host API for tests.** `widget_bounds` gives no clip rectangle, there is no iterator
  over the open screen's nodes, and there is no call to redraw the whole screen. The harness works
  around all three.
- [ ] **A user guide:** what the face shows, the swipes and button presses, moving from InfiniTime
  or wasp-os, updating with Gadgetbridge or ITD, and the recovery hold, with simulator screenshots.

### Not started

InfiniTime has these and NeoTime has nothing for them yet:

| Area | What InfiniTime has |
|---|---|
| Board and system loop | sleep and wake of the whole system, wake locks, a system monitor |
| Heart rate | HRS3300 driver, PPG processing, background measuring, the Heart Rate service |
| Battery | the Battery service |
| Clock | the Current Time client |
| Notifications | Alert Notification service and client, Immediate Alert, calls |
| Other BLE services | Device Information, Music, Navigation, Simple Weather |
| System screens | System Info, the pass key, firmware validation |

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

### Bluetooth

- [ ] **The rest of the Gadgetbridge Bangle.js messages** in
  [companion-protocol.md](companion-protocol.md): alarms, steps and HTTP. `settings`, `notify`,
  `notify-`, calls, music, weather, find, vibrate and status are served.
- [ ] **A developer console over BLE,** to replace the fork's Python REPL for uploads, time setting
  and resets.

### UI toolkit

- [ ] **Cards:** rounded-rectangle cards as the layout every app uses.
- [ ] **Widgets:** status bar, battery meter, clock, notification bar, scroll indicator, button,
  toggle, checkbox, slider, spinner, stopwatch and confirmation view.
- [ ] **App icons:** monochrome Font Awesome icons per app.
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

## Gaps against PinetimeLite

What [PinetimeLite](https://github.com/joaquimorg/PinetimeLite), an InfiniTime fork built for long
battery life, has beyond the items above, compared with its `developLite` branch at `a91eeee`
(version 0.7.0) on 2026-10-11. Its six watch face designs are already ported; see
[faces.md](faces.md). Its apps (steps, weather, motion, flashlight) are covered by the ports in
[apps.md](apps.md). Its weather in Alert Notification messages and its own file service are spoken
only by its Gadgetbridge fork and web app, so they are not worth copying.

### Watch faces

- [ ] **Double tap on a watch face puts the watch to sleep,** whichever face is showing.
- [ ] **Quick face switching.** PinetimeLite moves to the next face on a long press of any face.
  Decide whether NeoTime has a gesture for this, since several faces use the long press for their
  own settings.

### Quick settings

- [ ] **A quick settings screen** on a swipe down from the watch face, as
  [app-reference.md](app-reference.md) already promises: the time and battery at the top, then
  brightness, the flashlight, a switch that silences the vibration for notifications, and a
  shortcut to settings.

### Notifications

- [ ] **An icon and colour for each kind of notification:** email, missed call, message, calendar
  and the rest, so the kind shows before the text is read.

### Developer tools

- [ ] **Screenshots over BLE.** PinetimeLite draws the current screen into the external flash and
  lets the phone read it back as RGB565. It would let the docs and bug reports show the real watch.

### Firmware updates

- [ ] **Update from a web page** with Web Bluetooth, as PinetimeLite's web app does: set the clock
  and send firmware and resources without installing anything.

## Other firmware to study

Not compared yet. Look through each for features worth bringing to NeoTime.

| Firmware | What it is |
|---|---|
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
