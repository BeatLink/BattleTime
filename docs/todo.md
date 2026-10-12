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

- [ ] **Roll back a firmware on the watch.** A firmware that never confirms is put back at the next
  reset in the emulator; it has not been tried on a watch.
- [ ] **Run the power-cut sweeps** in `tests/emulator/migration.robot` (tagged `long`), which have
  never run on the final reloader. They take hours, so check the runner's memory early.
- [ ] **Check why wasp-os's update mode gives way to the reloader** about 4 seconds after the
  button escape in the emulator, with no reset between. Its advertising never times out, so this may
  be the emulator's SoftDevice; if a watch does the same, the escape leaves the reloader running.
- [ ] **Run the DFU service against more clients:** wasp-os's `dfu.py` works on the watch; the
  companion app and Gadgetbridge are next.
- [ ] **Async flash in the firmware.** Flash writes block the Bluetooth host for up to a sector
  erase at a time, because the display shares the SPI bus and its driver is blocking.
- [ ] **Signed updates**, through embassy-boot's ed25519 check in `UpdateReceiver::finish`.
- [ ] **A Gadgetbridge entry for NeoTime.** Gadgetbridge recognises InfiniTime by its advertised
  name; NeoTime advertises as "NeoTime" and needs a device coordinator of its own, or to be
  matched by the PineTime one.

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
- [ ] **Resource cache for external flash.** A small fixed area of RAM with two or three slots,
  reused least recently used first. Each slot is filled by one DMA read of a whole resource, such
  as a font or picture from a resource package or a handler from `/apps`, so each costs one SPI
  transaction instead of many small reads.

## Apps and faces

Nothing open.

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
- [ ] **Reset reason and crash record.** Read and clear `RESETREAS` at start, and keep the panic
  line or fault address in RAM the next start can read, then show both on the status page or in
  the diagnostics service.
- [ ] **Warn when a part fails at start.** InfiniTime shows a warning screen when the touch
  controller does not start; NeoTime shows nothing.
- [ ] **The version from the build.** The version is typed into `main.rs`. Take it from
  `Cargo.toml` and add the git commit, for the status page, a system info screen and the Device
  Information service.

### Display

- [ ] **Try the always-on screen on the watch.** Turn on Always on in the Settings app and
  check that the dim level is steady, without flicker, that the face keeps time, and how much
  battery a night costs. It has only been built, not run.
- [ ] **Wake the screen for an update's progress.** The status page draws over a dark or dimmed
  screen, so an update's progress cannot be read until the screen is woken.
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

- [ ] **Callers.** Buzz for notifications, alarms and timers.

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

### Battery

- [ ] **A steady battery reading.** Show 100 % and not charging once the charger stays connected
  but stops charging, let the percentage only rise on the charger and only fall on battery, cap it
  at 99 % while charging, and use InfiniTime's measured voltage curve. Keep the voltage for a
  battery screen.
- [ ] **A battery screen** with the charge, the voltage and whether it is charging, full, low or
  critical, as InfiniTime's battery info shows.

### Clock

- [ ] **Time zone and summer time.** The clock holds local time with an offset of 0. Keep UTC plus
  the zone and summer time offsets, set from the Local Time Information characteristic as
  InfiniTime does, so apps can show UTC and other zones.
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
- [ ] **Tell apps when a phone connects.** `is_phone_connected` always reads false and no `phone`
  event is sent, so the digital face's Bluetooth icon never shows and phone apps cannot say "not
  connected". Set it on connect and disconnect, and wake the shell on both.
- [ ] **Finish the Current Time service.** Reading 0x2A2B returns zeros instead of the watch's
  time, and the Local Time Information characteristic (0x2A0F) is missing. InfiniTime serves both.
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
- [ ] **Deliver sensor and phone events to apps.** The shell pushes only step counts. Push
  acceleration for the motion app, shake for dice, and heart rate, music, navigation and weather
  once those services exist.
- [ ] **Build every ported app and face into the firmware.** The watch carries 9 apps and only the
  digital face; the rest exist only as source. Choose the set at build time, as InfiniTime's
  `ENABLE_USERAPPS` and `ENABLE_WATCHFACES` do, and lift the launcher's 16-row limit.

### UI toolkit

- [ ] **Double line buffers with DMA,** so one is filled while the other is sent. InfiniTime uses
  two of 240 x 4 pixels. This also fixes the text edges listed above.
- [ ] **Fonts and pictures** built into the firmware, and more loaded from the external flash.
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
- [ ] **Force a rollback from the bootloader.** InfiniTime's bootloader 1.0.0 rolls back even a
  confirmed firmware on a shorter button hold than recovery. NeoTime only has the recovery hold.
- [ ] **Update progress as a bar with the result.** The status page counts kilobytes. Show a bar
  and a percentage of the image size, then a pass or fail, as InfiniTime does.
- [ ] **Give up on a stalled update.** InfiniTime aborts an update when no packet has arrived for
  10 seconds; NeoTime only aborts when the link drops.
- [ ] **A swap progress bar on the bootloader screen.** It says "Installing firmware" for up to 30
  seconds with nothing moving. Its 16 KB has about 2 KB to spare.
- [ ] **Cut the power during the updater's write** in the emulator, to measure the window and show
  that every cut outside it leaves a bootloader that starts.
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
| Notifications | Alert Notification service and client, Immediate Alert, notification list, calls |
| Other BLE services | Device Information, Music, Navigation, Simple Weather |
| System screens | System Info, Battery Info, Notifications, the pass key, firmware validation |

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
  [companion-protocol.md](companion-protocol.md): notifications, dismissals, music, weather, find
  phone, alarms, steps, status, calls and HTTP. `settings` is served.
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

### Battery

- [ ] **Charging screen.** Wake when the charger is plugged in or taken off, and show the charge
  with a filling bar until the screen times out.
- [ ] **Low battery alert.** Below 10 % and not charging, wake once and show a warning screen.

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
