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

- [ ] **Step history on the PineTime.** It has run only in host tests and the test watch. On the
  watch, check that:
  - walking fills today's hours, and a file appears in `/steps/<year>` within six minutes;
  - a restart keeps the day, the week and the trip count, losing at most six minutes;
  - local midnight moves today to yesterday with the steps app closed;
  - the phone setting the clock after a restart reads the right week back;
  - the slot writes every six minutes do not upset the Bluetooth link.

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
  - the controller's sleep between events holds a connection at both a 7.5 ms and a 4 s interval,
    so the 1.5 ms allowed for the 64 MHz crystal to start is enough on this watch's crystal;
  - the current drawn while advertising and while connected, against the old build that kept the
    crystal and TIMER0 running.

- [ ] **Settings on the PineTime.** The Settings app, the settings store and the companion's
  `settings` keys have run only in the simulator and the emulator. On the watch, check that:
  - each switch and choice in the Settings app takes effect: brightness at once, the screen
    timeout on the next idle, 12 or 24 hours and units on the face and apps;
  - the always-on screen dims at a steady, flicker-free level instead of turning off;
  - chimes buzz on the hour and half hour, and stay silent in do not disturb;
  - with alarms in do not disturb off, an alarm stays silent in do not disturb;
  - each wake choice works on a dark screen: a tap, a double tap from the sleeping panel, and only
    the button, with the panel asleep;
  - Bluetooth off drops the phone and stops advertising, on brings it back, and a restart turns
    it on again;
  - with updates off a phone's DFU is refused and the status page says "Updates are off";
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

- [ ] **The notes app on the PineTime.** It has run on the emulated firmware with a 30 KB note in
  littlefs, opened from the launcher and scrolled to line 115, but not on the watch. With the
  companion app's Notes screen, check that:
  - a chosen file reaches `/notes/note.txt`, and an edit to it is sent again within a minute;
  - a font chosen in the companion app shows the next time the app opens;
  - a long note scrolls smoothly to its last line, with no line drawn twice or out of place;
  - the stack keeps its margin with the app open, which in the emulator matches the launcher's.
- [ ] **The remote app on the PineTime.** It has run only in the test harness. With the companion
  app's remote screen, check that:
  - buttons chosen in the companion app reach `/remote/buttons` and show the next time the app
    opens or the screen turns on;
  - each tap's `{"t":"app","ch":"remote",...}` line reaches the phone, which runs that button;
  - the stack keeps its margin while a tap's line is sent.
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

- [ ] **Alert Notification from BlueZ.** The New Alert write (`0x1811`/`0x2A46`) is tested only on
  the computer; nothing has written one over Bluetooth yet. First through the emulator's Bluetooth
  bridge with `gatttool -i <bridge>`, then on the watch, check that:
  - a write of `05 01 00` then `Sam`, a zero byte and `Hi` shows an SMS notification from Sam that
    buzzes twice and wakes the screen;
  - a write longer than 20 bytes arrives whole once BlueZ has raised the MTU, and the write is
    answered without an error, since the watch reads it without storing it;
  - ITD's `itctl notify` and Amazfish show their notifications the same way.

## Bluetooth controller

See [bluetooth.md](bluetooth.md) for the details of each.

- [ ] **Encryption and pairing.** Answer `LL_ENC_REQ` and encrypt packets with the chip's CCM and
  ECB peripherals, so phones that insist on pairing can connect.
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
- [ ] **An update over Bluetooth in the emulator.** The update suites write images straight into
  flash, so no emulator test sends a firmware through the DFU service. Send one over the
  PineTimeEmulator Bluetooth bridge from a raw-HCI client, which also proves the controller's sleep
  between events holds through a long transfer.
- [ ] **Async flash in the firmware.** Flash writes block the Bluetooth host for up to a sector
  erase at a time, because the display shares the SPI bus and its driver is blocking.
- [ ] **Signed updates**, through embassy-boot's ed25519 check in `UpdateReceiver::finish`.
- [ ] **Run the wasp-os emulator tests in CI.** The `Emulator suites` job skips the tests tagged
  `wasp-os` (the rehearsals and the wasp-os migration), since they start from a wasp-os bootloader
  build with Nordic's S112 that only a local checkout has. Package that build, or its parts, so CI
  can run them too.
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
  `status` and the `music`, `call`, `findPhone` and `app` lines are served (`services/companion/src/phone.rs`).
  - `alarm` (set or report the alarms), `steps` (a day in 240 six-minute slots, read from the
    step log with `neotime_steps::log`) and `http` (answers to an app's request, and the watch's `http`
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

- [ ] **More of the shared library's functions.** `CallLibrary` runs the pure functions in
  `LIBRARY_FUNCTIONS`. Still open:
  - The companion does not check the library version before installing, so an app too new for
    the watch installs and then never opens.
  - Functions that change state, taking the variables by number as `copy array` names its arrays.
  - Calls from the faces and apps that still compute the same things by hand. One-line helpers
    such as `(x + 1) % n` stay inlined, since a call costs as many bytes.
- [ ] **Native widgets with their logic built in.** Every widget node costs 160 to 240 B of the UI
  heap wherever its record lives, so a composite drawn by the firmware as one node saves more RAM
  than a library component. In the firmware, which has 112 KB free:
  - `AppGrid`, the installed apps as a grid of icons, beside the `AppList` the launcher uses.
  - The other faces' own status rows on `StatusIcons`: analog, lite-analog, lite-digital,
    lite-rings, infineat, pinetime-style and pride-flag still build theirs from icons, in
    colours and orders of their own; `StatusIcons` would need a colour for each icon.
  - A `ring(title, icon, until)` system alert with dismiss and snooze events, with the vibration
    and the stop after a time limit, for the alarm, timer and pomodoro.
  - A digit-entry mode for `ButtonMatrix`, for the calculator and pomodoro.
  - The other faces' step, heart and weather rows on `StepsBadge`, `HeartRateBadge` and
    `TemperatureBadge`: infineat, lite-digital, lite-weather, terminal and pinetime-style.
- [ ] **Comparing with a constant array.** `arrays_equal` takes only signals, so puzzle15's check
  against its constant solved board still loops.
- [ ] **Library functions that change state.** What is left of the shared logic writes state, so it
  waits on library functions that take variables by number: `elapsed_clock` (start, pause and
  reset; stopwatch, sports, metronome, heart), `countdown` (timer, pomodoro) and `record_best`
  (twos, snake, puzzle15). The pure helpers that were on this list either moved into the library
  or turned out to be one-liners that a call would not shrink.
- [ ] **Library components.** Widget trees with props, for layouts that need no native logic:
  `ActionRow` and `PlayPauseButton` (7 apps), `ToggleScreen` (phone-finder, disable-bluetooth,
  flashlight, beacon), `SettingSwitch` and `ChoiceList` (settings), `CycleChooser` and
  `DoneButton` (infineat, pride-flag, pinetime-style), `GameOverPanel`, `ScoreHeader`,
  `EmptyState`, and `ClassicDial` with `DateWindow` (analog, lite-analog).
- [ ] **Library strings and palettes.** Start, Stop, Reset, Cancel, Dismiss, Snooze, Once, Every
  day, Weekdays, Weekends, Game over; InfiniTime's 18 colours, the resistor code and a rainbow; a
  theme colour for the `#b0b0b0` that 9 apps write out.

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
- [ ] **Measure the alarm's stack again.** About 11.3 KB is now left for the stack, after the Alert
  Notification service took 160 B more, and the alarm's last peak of 10.5 KB would leave under the
  2 KB minimum. `tools/measure_stack.py` cannot open the
  alarm, so measure it with the alarm installed and open, in the emulator or on the watch.
- [ ] **Find large static RAM.** List `.bss` and `.data` by size with `nm --size-sort` and check
  for oversized embassy task state or tables that should be `const`.

## Apps and faces

Found by the harness's snapshots and quality checks; see [testing.md](testing.md).

- [ ] **Calculator's 7 key is always highlighted,** because the compiler cannot pass labels to
  `highlighted` (see [compiler.md](compiler.md)).
- [ ] **Lines are never drawn.** rlvgl's default `stroke_line_aa` clips to a rectangle whose right
  and bottom edges are at -1, so every line is clipped away: the analog faces show no hands or
  ticks, and charts, such as the steps app's day and week, show only their points. Override it in
  `neotime-ui`'s renderer and `FadedRenderer` with the screen as the clip.
- [ ] **Terminal face overflow.** Its last line falls off the bottom of the screen and
  "[STAT] Connected" is cut in half.
- [ ] **Count the heap's peak exactly.** The emulator measures the allocator's bytes in use and
  the highest heap byte touched, which misses allocations filled with zeros. A peak counter in the
  allocator, read over the diagnostics characteristic, would check the heap estimate against every
  app's worst moment rather than its first screen.

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
  bus stops touch and steps for good. InfiniTime also turns the TWIM off between transfers.
- [ ] **Warn when a part fails at start.** InfiniTime shows a warning screen when the touch
  controller does not start; NeoTime shows nothing.

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

- [ ] **Double taps in apps on the watch.** The firmware starts the panel with double taps off, so
  `onDoubleClick` never fires in an open app; only waking by double tap uses them. Turn them on
  when the open screen wants them.

### Vibrator

- [ ] **Check `wake` before an alarm rings through do not disturb.** Keep app permissions in the
  image so the watch can check `wake` instead of trusting any app that plays the `alarm` pattern.
- [ ] **Try the buzzes on the watch:** a phone notification buzzes twice and stays silent in do
  not disturb, and the alarm pattern still rings there. Check too that a notification on a dark
  screen turns it on into the notifications app, that a `notify-` from Gadgetbridge removes it, and
  that the heap holds with five kept beside the alarm app.

### Side button

- [ ] **Try the gestures on the watch:** a click, a double click, a long press and a 2 second
  hold each do what docs/features.md says, and a 5 second hold still resets into the bootloader.
- [ ] **Double clicks and 2 second holds for apps.** `onButton` hears only presses and long
  presses; the shell keeps the other two.

### Accelerometer

- [ ] **Raise to wake and lower to sleep,** in software on `read_acceleration`.
- [ ] **Shake to wake,** with an adjustable threshold.
- [ ] **Motion service over BLE,** sending steps and raw x, y and z to the phone.

### Clock

- [ ] **Set the date and time on the watch,** with rollers for the date and the time (with AM/PM on
  a 12 hour clock), for when no phone has set the clock.
- [ ] **The NeoTime Companion app should write the time zone.** It writes only Current Time
  (0x2A2B), which sets local time, so the watch's UTC is off by the zone until 0x2A0F is written
  too. The fix is in the NeoTime Companion repository.

### Settings

- [ ] **The rest of InfiniTime's settings screens:** Watch face, Date and time, Shake calibration,
  Firmware, Heart rate and About. Each needs its feature first, such as choosing among installed
  faces or setting the clock by hand.

### Bluetooth

- [ ] **Pairing with a passkey.** Secure Connections with a 6-digit passkey shown on screen, as
  InfiniTime does. Needs encryption in the controller first.
- [ ] **Bonding** saved to the file system and restored at start.
- [ ] **Privacy:** resolvable private addresses.
- [ ] **Light the screen when a phone connects or leaves.** The shell redraws `is_phone_connected`
  and sends the `phone` event, but a dark screen stays dark; turn it on briefly, outside do not
  disturb, so the wearer sees the link change.
- [ ] **Advertise like InfiniTime.** The firmware advertises every 160 ms all the time. InfiniTime
  advertises fast for 30 seconds after start or a disconnect and then about once a second, puts the
  heart rate and DFU service UUIDs and the transmit power in the advertisement, and the name in the
  scan response, which some phone apps filter on.
- [ ] **Match InfiniTime's GAP appearance,** 0x00C2 (a watch worn on the wrist), where NeoTime
  says Generic Watch, 0x00C0.
- [ ] **A larger ATT MTU.** The firmware allows 64 bytes where InfiniTime asks for 256. File
  transfer, long track names and notifications need longer writes.

### App runtime

- [ ] **Run closed apps on the watch.** The shell runs only the open app and never calls
  `open_in_background`, so an alarm or timer rings only while its app is open. Run each installed app's background subscriptions, set
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
- [ ] **More library icons:** brightness levels, sleep, notifications on and off, list,
  home, eye, ban, phone off, volume mute, tachometer, paintbrush, paddle, map, shield, and the
  moon and night rain weather icons. Quick settings and the ports need them.
- [ ] **UI design guidelines.** Add InfiniTime's rules to `app-reference.md`: buttons at least 50
  pixels high and on the bottom edge, room for a page indicator and top bar, switches twice as
  wide as tall, and black backgrounds.

### Apps and faces

- [ ] **Try the system info app on the watch.** It has run only in the harness and the simulator.
  Install the new shared library first, which carries its `info` icon, then check the version,
  the uptime after the phone sets the clock, the reset reason after a button-hold reset, and the
  free storage before and after an install.
- [ ] **More in the system info app:** the free heap, the Bluetooth address and the display's
  state, as InfiniTime's System Info screen shows.
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
- [ ] **The rest of the music values.** InfiniTime's music service also carries the album, the track
  number and total, playback speed, repeat and shuffle, and moves the position on at the playback
  speed while a track plays. The runtime has no value for any of them.
- [ ] **Music disc artwork.** InfiniTime's music app shows a disc that spins while a track plays.

### Simulator

- [ ] **Simulated parts** for touch, the accelerometer, the battery and Bluetooth, plus
  notifications and heart rate once those exist.
- [ ] **GIF capture,** as InfiniSim has.

### External flash and storage

These live on the `worktree-bootloader`, `worktree-flash-async` and `worktree-updates` branches.

- [ ] **Deep power-down in sleep.** Put the flash chip to sleep when the watch sleeps.
- [ ] **Use the file system:** save alarms and the BLE bond there.
- [ ] **Resource packages:** fonts and pictures installed from a zip over BLE, with a list of
  obsolete files that an upgrade deletes, as InfiniTime's `resources.json` has.
- [ ] **Report failed flash writes.** After each erase and page program, read the flash's security
  register and return an error when its erase-fail or program-fail bit is set, so littlefs moves
  the block instead of trusting it.
- [ ] **Room for the step log.** Each day's file past 12:48 takes a 4 KB block, so the log keeps
  every day but fills the file system's free 2.8 MB in about two years. Pack finished days, such
  as a month to a file, or drop the oldest when space runs low. Until the clock is set after a
  restart, steps are logged under the firmware's build date.

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
- [ ] **Updates on only until the next reset,** as a third choice beside the on and off switch.
- [ ] **Show the bootloader version** in the system info app. The bootloader keeps no version
  yet, so it needs one at a fixed place the firmware can read, and an update of the bootloader on
  the watch.

### Developer tools and docs

- [ ] **Measure the stack with any app open.** `tools/measure_stack.py` can only swipe to the
  launcher, so the alarm's 10.5 KB peak cannot be measured again, and neither can the system info
  app's. Let it install and open a named app in the emulator.
- [ ] **A DFU upload tool in `tools/`** that sends a zip over BlueZ, as wasp-os's `dfu.py` does,
  including type `0x08` for the recovery slot, which no client sends yet.
- [ ] **Flashing a watch over SWD.** One image with the bootloader and the firmware, a way to fill
  the recovery slot on a watch flashed this way, and a page on probes, the SWD pins and RTT logs.
- [ ] **Releases.** A version scheme, tagged releases carrying `dist/`, and a page saying what
  each file is for.
- [ ] **How to measure memory.** Say in [memory-map.md](memory-map.md) how each figure is measured
  on the watch: flash by crate and symbol, the stack's high-water mark, and heap peaks.
- [ ] **Firmware logic where tests reach it.** Coverage measures only the workspace. Move logic out
  of `firmware/` into workspace crates, as `boot` and the services already are, so host tests
  cover it.
- [ ] **Harness gaps.** Open a named screen directly, so screens behind icon-only buttons, such as
  the alarm's edit and ring screens, get smoke and snapshot coverage; read the key labels of a
  `ButtonMatrix`, so `tap "7"` works on the calculator; match text spread over neighbouring
  labels, such as a face's "09:00"; set text system values from scripts; give `notify` an
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
| Clock | the Current Time client |
| Notifications | the Alert Notification client, Immediate Alert, the call screen for a New Alert call |
| Other BLE services | Music, Navigation, Simple Weather |
| System screens | the pass key, firmware validation |

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

### Accelerometer

- [ ] **Write the axis remap to the chip,** so its own motion features see the watch the right way
  up. The driver only remaps in software now.

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
- [ ] **Grid and list launchers,** chosen in settings. InfiniTime's grid shows 6 apps a page,
  remembers the last page, and greys out apps and faces whose resources are missing.

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
  shortcut to settings. InfiniTime's switch has a third mode, sleep, which also turns off the wake
  gestures and the always-on screen.

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
