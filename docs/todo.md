# To do

Work that is known to be needed but not done yet.

When an item is done, remove it here. If it is something the user sees, add it to
[features.md](features.md); if not, describe it in [architecture.md](architecture.md) or the
document for its part instead.

## Hardware tests

- [ ] **Paging on the PineTime.** `Pages` (2f9524a) and the system and steps apps built on it
  (5957275) have run only in the harness and the emulator; the watch still runs 3a5abe9. Flash a
  build with them and check on the watch that a swipe left or right wipes to the next page and
  never skips one, even a long slow swipe, which the panel repeats as it goes, the
  page bar marks the page on show, a sideways drag leaves the system page's list still, swipes past
  the first and last page step round the quick apps, and a charger event opens the battery page.

- [ ] **Pomodoro on the PineTime.** The pomodoro rebuilt around three intervals (4d27f48a) has run
  only in the harness and the browser preview. Check on the watch that it opens on the countdown,
  that the three length pages are reached by swiping right and their - and + buttons set the
  lengths, and that a round runs work and short breaks in turn with a long break after the set
  number of work sessions, buzzing and waking the screen between each.

- [ ] **Touch panel on the PineTime.** The firmware runs `neotime-touch` on the watch, but these
  checks have not been made:
  - `start` succeeds and `read_chip_info` gives a sensible chip ID (0xb4 or a near value);
  - taps, long presses and swipes in all four directions, found by `GestureFinder` from the
    finger's path, come through quickly and as the right gesture, short flicks included, and a
    scroll in a list never turns into a swipe and keeps following the finger past 40 px;
  - a flicked list glides on and stops naturally, a slow drag lifted gently does not glide, and a
    tap on a gliding list stops it without opening the row;
  - a quick tap is never lost to the 30 ms shortest tap, and a tap that wakes the screen opens nothing;
  - reports keep coming while a finger is held still, or the 300 ms lost-lift timeout cuts long
    presses short; measure the gap between reports and set the timeout to two or three of them;
  - a drag streams reports while the finger stays down, and the finger lifting is reported;
  - the corners and edges report positions near 0 and 239, or edge swipes need a margin;
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
  - raise to wake, lower to sleep and shake to wake act on a real wrist as in the emulator, the
    default shake strength of 150 is neither too eager nor too hard, and reading ten times a
    second while the screen is dark costs little battery.

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
  - with updates off a phone's DFU is refused and the status page says "Updates are off", and an
    app install is refused; with Temporary both go through until the next restart;
  - on a firmware sent and not yet confirmed, the System app's Confirm keeps it and Roll back
    restarts into the one before;
  - a preset theme recolours the open app and the next one opened;
  - settings survive a restart, from `/settings/system` and `/settings/theme`;
  - a `settings` message from the companion app changes them and the reply lists every key.
- [ ] **Files over BLEFS on the PineTime.** The file service has run only in the emulator, against a
  raw HCI client. On the watch, with the companion's `neotime files` commands, check that:
  - the phone reaches an MTU of 124, so a write of up to 121 bytes is taken whole;
  - listing, making a folder, writing a file, reading it back, moving it and deleting a folder work;
  - a long listing arrives complete, with no notification lost while packets are short;
  - free space reads back through the companion's scratch-file write.
- [ ] **Installing apps on the PineTime.** On the watch, firmware ae36259 took the shared library,
  the launcher, a face and every app through `neotime apps install` and listed them with `apps`.
  Still check on the watch that:
  - the apps appear in the launcher without a restart;
  - hiding, showing and removing one updates the launcher;
  - an image with a bad id or a library that fails its check is refused with an error.
- [ ] **The Software app and memory checks on the PineTime.** They have run only in the emulator.
  On the watch, check that the Software app lists every app with its memory, that its switches turn
  apps off and on, that the launcher and the last face refuse to turn off, and that `neotime apps
  list` prints each app's memory.
- [ ] **Bluetooth stopped advertising after an update.** Firmware 94e78c0, the first with the
  crystal and TIMER0 sleeping between radio events, stopped advertising on the watch soon after it
  was flashed; nothing was connected, and only the recovery firmware brought it back. Firmware
  ae36259, with the same change, advertised and installed every app. Find what stops advertising,
  starting with the sleep path.
- [ ] **A stack overflow on the PineTime.** Before the stack guard, an overflow in a packages
  command left the watch connected but silent, with its touch screen dead; the emulator kept
  touch working, so the dead touch was never explained. On the watch, check that an overflow now
  ends in the crash screen.
- [ ] **Reset reasons and crash record on the PineTime.** They have run only in the emulator. On the
  watch, check that:
  - the status page reads "Reset: power on" after the battery is reconnected, "Reset: watchdog"
    after a button hold, and "Reset: software" after an update;
  - when a panic or a HardFault happens, it shows on the next start's status page and in the
    diagnostics characteristic's last-restart words, and a panic's addresses lead to its caller;
  - an ordinary start after that shows the crash no more.
- [ ] **The crash screen on the PineTime.** It has run only in the emulator, and saving a debug report
  only against the companion's mock watch. On the watch, check that a crash shows the crash page
  after the restart, that Restart and the side button go on to the face, and that `neotime debug`
  saves a report naming the crash and the open app and then takes the watch to its face.
- [ ] **A debug report saved over the emulator's Bluetooth bridge.** The write that dismisses the
  crash is tested only against the companion's mock; `tools/packages_over_hci.py` could write `01` to
  the diagnostics characteristic over `pinetime-engine --bluetooth`.
- [ ] **Allocation failures inside an open app.** The app host allocates with `Vec` and `Box`, whose
  failures abort, so an app that outgrows the heap after it opened still ends in an out-of-memory
  crash and the crash screen rather than closing with a message. Catching it would need fallible
  allocation through the host's nodes, text and lists.

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
- [ ] **The QR code app on the PineTime.** It (9e8f7b13) has run only in the test harness and the
  companion's mock watch. With the companion app's QR code screen or `neotime qr`, check that:
  - `/qr/list` and the codes in `/qr/<slot>` arrive, the list shows them the next time the app
    opens, and a tapped code is read and drawn without a long pause;
  - another phone scans a short link, a Wi-Fi code and a code over 74 modules;
  - drawing the largest code stays quick enough for the watchdog.
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
- [ ] **Battery line and charge floor on the PineTime.** The bootloader's battery line and the
  updater's 3.7 V floor before a bootloader write (75f9bcde) have run only in the emulator. Send
  `dist/bootloader-updater.zip` with `neotime flash`, then on the watch check that:
  - off the charger the updater says "Plug in the charger" and its line reads "unplugged";
  - on the charger the line reads "charging" with a charge close to the system app's, and the
    updater writes the bootloader once the battery reads at least 3.7 V;
  - the new bootloader's blue "Keep holding for the menu" page shows the same line.

- [ ] **Alert Notification from BlueZ.** The New Alert write (`0x1811`/`0x2A46`) is tested only on
  the computer; nothing has written one over Bluetooth yet. First through the emulator's Bluetooth
  bridge with `gatttool -i <bridge>`, then on the watch, check that:
  - a write of `05 01 00` then `Sam`, a zero byte and `Hi` shows an SMS notification from Sam that
    buzzes twice and wakes the screen;
  - a write longer than 20 bytes arrives whole once BlueZ has raised the MTU, and the write is
    answered without an error, since the watch reads it without storing it;
  - ITD's `itctl notify` and Amazfish show their notifications the same way.
  - a write of `03 01 00` then `Sam` opens the call screen for Sam and rings until Reject or the button.

## Bluetooth controller

See [bluetooth.md](bluetooth.md) for the details of each.

- [ ] **Encryption and pairing.** Answer `LL_ENC_REQ` and encrypt packets with the chip's CCM and
  ECB peripherals, so phones that insist on pairing can connect.
- [ ] **Peripheral latency.** Skip the connection events the central allows the watch to skip.
- [ ] **Faster transfers.** Data length extension, the 2 Mbit PHY and channel selection algorithm
  #2, mainly to speed up firmware updates.
- [ ] **Connection parameter request** from the peripheral. TrouBLE falls back to L2CAP meanwhile.
- [ ] **Watch the Bluetooth host task.** The liveness watchdog sees only the radio interrupt
  count, so a stalled TrouBLE host task with a working radio still feeds it.
- [ ] **A crash record for a watchdog reset.** A hang that the liveness watchdog resets leaves no
  crash record, so no crash screen follows; record which task stalled before letting it bite.
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
- [ ] **The shell's store calls without blocking.** The storage task lets the CPU go while the flash
  works, but the shell calls the store directly and polls the chip, so a write it makes, such as an
  app's saved state, blocks every task for up to a sector erase. Make the shell's loop await its
  store calls, and the app host's `read_file`, which `TextFile` and `file_bytes` use while drawing,
  `async`; the open path's recursion (`open` and `open_failed`) needs reworking first.
- [ ] **Signed updates**, through embassy-boot's ed25519 check in `UpdateReceiver::finish`.
- [ ] **Choose what runs over Bluetooth.** Rolling back to the previous firmware or installing the
  recovery firmware needs the bootloader's button menu. A companion command such as
  `neotime boot previous|recovery` could do the same from the firmware: check the image's header and
  CRC, copy it into the update slot (`copy_previous_into_update_slot`, `copy_image_into_update_slot`),
  mark a swap and restart, so it runs on trial like any update.
- [ ] **Find why the first `slots` after a flash timed out.** On 2026-10-13 the first
  `neotime slots`, sent right after `neotime flash` and `neotime time`, got no reply; every later
  request answered at once and it did not happen again.
- [ ] **Install recovery from a resized slot on the watch.** After `neotime slots fit` shrank the
  watch's recovery slot to 168 KB, the firmware read the record back, but the bootloader menu's
  Install recovery from that slot has only run in the emulator; it needs a button hold.
- [ ] **Clippy fails on master.** CI's `Lints and types` and the four `Watch lints` jobs are red at
  04005694: eight operations with no effect sit in `neotime/firmware/src/main.rs` (lines 651, 683,
  690, 692, 706, 714, 721 and 723).
- [ ] **Run the stack overflow test in the emulator.** `crash_record`'s "A Stack Overflow Faults At
  The Guard And Is Reported" skips, since PineTimeEmulator has no memory protection unit yet; it
  runs once the emulator models one.
- [ ] **Read the bootloader in the updater with assembly loads.** `bootloader-updater`'s
  `installed_crc` reads `BOOTLOADER.start` (address 0, the null pointer) through `Nvmc`, which is
  undefined behaviour; the firmware's same read let the compiler delete most of the firmware. Use
  loads like `status::BootloaderPages` in the firmware.
- [ ] **A Gadgetbridge entry for NeoTime.** Gadgetbridge recognises InfiniTime by its advertised
  name; NeoTime advertises as "NeoTime" and needs a device coordinator of its own, or to be
  matched by the PineTime one.

## UI toolkit

- [ ] **Measure the UI heap on the watch.** It is 20.25 KB, shared with the system's 10 KB; so
  far it is measured only in the emulator, where the alarm's editor touched 13,188 B of the 31,744 B
  the heap had then. Count what a typical screen allocates on
  the watch, then update the "UI heap" row in
  [memory-map.md](memory-map.md) and the size in `tools/draw_memory_maps.py`, and rerun the script.
- [ ] **App screens in the simulator.** The simulator still shows hand-drawn demo pages. Draw
  real app screens there through `ScreenRenderer`.
- [ ] **Touch input.** Turn `neotime-touch` gestures into the app host's touch events.

## App runtime

- [ ] **Profiles.** Sets of apps such as work, gaming or comms, picked from the companion app. A
  profile is a small file in external flash listing app ids, their order, the watch face and which
  subscriptions are on. Switching profiles only reads files, so it writes nothing to flash.
- [ ] **Temporary apps the companion sends by itself.** The watch runs temporary apps from `/temp`
  and the companion can send one by hand (`neotime apps run`, `neotime ask`), but nothing sends a
  call screen, a custom notification page or a game score when a call, a notification or a score
  arrives on the phone, and the companion app has no screen for choosing them.
- [ ] **Try temporary apps on the watch.** They have run only in the emulator. On the watch, check
  that `neotime ask` opens the prompt over the face and over another app with the screen dark and
  lit, that Yes and No reach the computer and go back, that a second `neotime ask` reuses the cached
  image, and that turning Bluetooth off on the computer closes the prompt.
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

## Shared library and smaller apps

The apps and faces hold about 90 KB of images, down from 197 KB, now that the compiler calls an
app's own functions and the shared library's, shares code that repeats with other constants, names
library icons, and hands whole-array work and composite widgets to the firmware (see
[app-runtime.md](app-runtime.md#building-apps-from-shared-parts)). What is left saves less each, so
each item says what it costs and what it would save.

- [ ] **A system ring alert.** A `ring(title, icon, until)` effect that the firmware shows over
  any screen, with Dismiss and Snooze sending events back to the app, the vibration, the side
  button stopping it, and the stop after a time limit. The alarm, timer and pomodoro each build
  their own ringing screen and logic today, and they differ: the alarm snoozes for ten minutes and
  stops after a minute, the timer stops on any tap, and pomodoro buzzes for ten seconds and
  then starts its next interval, so the alert needs a snooze length and an "ended by itself" event.
  A design change in the firmware shell; it removes each app's ringing screen and its timers.
- [ ] **Library components.** Widget trees with props kept in the library and named by number,
  for layouts that need no native logic: `ActionRow` and `PlayPauseButton` (7 apps),
  `ToggleScreen` (phone-finder, flashlight, beacon), `SettingSwitch` and
  `ChoiceList` (settings), `CycleChooser` and `DoneButton` (infineat, pride-flag,
  pinetime-style), `GameOverPanel`, `ScoreHeader`, `EmptyState`, and `ClassicDial` with
  `DateWindow` (analog, lite-analog). Needs widget records and their bindings to live across two
  images, with props passed into the library's expressions. Saves about 50 to 200 B of image an
  app; the widget nodes still cost the same RAM, so a native widget pays more where one fits.
- [ ] **Library strings and palettes.** Start, Stop, Reset, Cancel, Dismiss, Snooze, Once, Every
  day, Weekdays, Weekends, Game over; InfiniTime's 18 colours, the resistor code and a rainbow; a
  theme colour for the `#b0b0b0` that 9 apps write out. Strings need a third kind of text
  reference, to a library string, which the host resolves when it draws; each saves about 10 B an
  app.
- [ ] **Library functions that change state.** Functions taking the variables they write by
  number, as `copy array` names its arrays, such as `elapsed_clock` (start, pause and reset;
  stopwatch, sports, metronome), `countdown` (timer, pomodoro) and `record_best` (twos, snake,
  puzzle15). Measured first: each of these is two or three assignments in its app, which a call
  with its slot numbers would not shrink, so it pays only if a longer shared routine turns up.
- [ ] **Calls from apps that still compute shared things by hand.** Any app or face that repeats
  what a `neotime/lib` function does should call it. One-line helpers such as `(x + 1) % n` stay
  inlined, since a call costs as many bytes.
- [ ] **Comparing with a constant array.** `arrays_equal` takes only signals, so puzzle15's check
  against its constant solved board still loops; the compiler could compare against a constant
  directly.
- [ ] **More native widgets.** Every widget node costs 160 to 240 B of the UI heap, so a composite
  drawn as one node saves RAM where a library component does not:
  - `AppGrid`, the installed apps as a grid of icons, beside the `AppList` the launcher uses; it
    has no user until a grid launcher is wanted.
  - A digit-entry mode for `ButtonMatrix`. Only the calculator types digits, with millionths, a
    point and a sign, so it pays once a second app needs a keypad.
  - The other faces' status rows and their step, heart and weather rows: analog, lite-analog,
    lite-digital, lite-rings, infineat, pinetime-style, pride-flag, lite-weather and terminal. Their
    icons sit apart, in colours of their own, and analog swaps the battery for a plug while
    charging, so `StatusIcons` and the badges would need per-icon colours, places and that plug
    mode before they fit; each face would save one to three nodes.

## RAM

- [ ] **Stack margin with the alarm open.** An earlier measurement put the alarm at 10,516 B of
  12,436 B, with alarms set and a larger install. On 2026-10-13, with the display's strip buffers
  static, opening the alarm and its editor from a launcher listing only it peaked at 6,152 B of
  8,432 B, 2,280 B spare, the same as opening any app; find the deeper case again and measure it.
  Opening an app is the deepest path: the app host's `load_into` and `run_handler` frames (860 B
  and 876 B) and the interpreter's under them are where to win stack back.

- [ ] **Smaller Bluetooth buffers.** Fewer and smaller packets in the pool and one connection take
  RAM from the 12 KB budget, at some cost to file transfer speed. The GATT table is already in
  flash; the Bluetooth task's state is 3.2 KB, TrouBLE's runner (1.6 KB) much of it, and the GATT
  server's 496 B are mostly TrouBLE's CCCD table for each client; shrinking those means patching
  `third_party/trouble-host`.
- [ ] **Find large static RAM.** List `.bss` and `.data` by size with `nm --size-sort` and check
  for oversized embassy task state or tables that should be `const`.

## Apps and faces

Found by the harness's snapshots and quality checks; see [testing.md](testing.md).

- [ ] **Terminal face overflow.** Its last line falls off the bottom of the screen and
  "[STAT] Connected" is cut in half.
- [ ] **Check the free heap before reading an image beside an open app.** A packages `install`, the
  launcher reading icons and opening an app all read a whole image while another app may hold the
  heap. The package manager now refuses images larger than an app's budget, but one inside it can
  still meet a heap the open app has mostly taken; read `LlffHeap::free` first and refuse instead.
- [ ] **Packages in the simulator.** The simulator gives no `packages` values, so the Software app
  shows an empty list there; feed it the apps it loaded, through `neotime-packages`'s checks.
- [ ] **Count the heap's peak exactly.** The emulator measures the allocator's bytes in use and
  the highest heap byte touched, which misses allocations filled with zeros. A peak counter in the
  allocator, read over the diagnostics characteristic, would check the heap estimate against every
  app's worst moment rather than its first screen.

## Display and SPI bus

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
- [ ] **The panel's power settings.** `Display::new` starts the ST7789 with its defaults. InfiniTime also
  sets the lowest panel and gate voltages, a VDV value that makes the first grey above black less
  of a jump, and its own porches and frame rate. Send the same commands after `Display::new`, then
  compare the current drawn and the dark greys on the watch.
- [ ] **Release the panel in sleep,** so the SPI bus can be turned off.
### Touch panel

- [ ] **Double taps in apps on the watch.** The firmware starts the panel with double taps off, so
  `onDoubleClick` never fires in an open app; only waking by double tap uses them. Turn them on
  when the open screen wants them.

### Vibrator

- [ ] **Check `wake` before an alarm rings through do not disturb.** Keep app permissions in the
  image so the watch can check `wake` instead of trusting any app that plays the `alarm` pattern.
- [ ] **Try the buzzes on the watch:** a phone notification buzzes twice and stays silent in do
  not disturb, and the alarm pattern still rings there. Check too that a notification lights a dark
  screen on its preview, which goes back by itself after 7 seconds, that a `notify-` from Gadgetbridge removes it, and
  that the heap holds with five kept beside the alarm app.

### Side button

- [ ] **Try the gestures on the watch:** a click, a double click, a long press and a 2 second
  hold each do what docs/features.md says, and a 5 second hold still resets into the bootloader.
- [ ] **Try the face's swipes on the watch:** down to the notifications and again to quick
  settings, up to put them away, down in the launcher, and left and right round the quick apps.
  They are tested on the host and in the harness only.
- [ ] **Edit the quick apps from the companion app:** a list of installed apps to pick and order,
  written to `/settings/quick_apps` one id a line. The watch reads it on every step round the ring.

### Accelerometer

- [ ] **Motion service over BLE,** sending steps and raw x, y and z to the phone.

### Clock

### Settings

- [ ] **The rest of InfiniTime's settings screens:** Shake calibration and About. Each needs its
  feature first.

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

- [ ] **Pushed events for closed apps.** Closed apps run their timers, alarms, daily times, clock
  changes and heart rate periods, but steps, step goal, motion, battery, phone, notification, call,
  music, weather, navigation and settings-change subscriptions reach only the open app. Each would
  load its listening closed apps through `neotime_app_host::closed_apps` as the timers do.
- [ ] **An emulator suite for closed apps.** A timer ringing while closed was measured by hand in
  the emulator ([memory-map.md](memory-map.md)); no suite under `tests/emulator` repeats it, so a
  later change to that path is not checked for its stack and heap.
- [ ] **Deliver the remaining sensor and phone events to apps.** The shell pushes steps,
  acceleration, shakes, heart rate and phone notifications. Push the other `motion` kinds (wrist raise, tap,
  double tap, still, moving), battery changes, calls, music, navigation and weather
  once those services exist.
- [ ] **Try acceleration and shakes on the watch.** The motion and level apps should follow the
  watch's tilt with the right axes and signs, a firm shake should roll the dice once, and walking
  should not. Tune `JOLT_THRESHOLD_IN_MILLI_G` if needed.

### Heart rate

- [ ] **Try the heart rate sensor on the watch.** It reads the emulator's pulse, but the filter
  chain's gain, the LED current and the no-skin limit were never tuned on a real wrist. Compare
  readings with InfiniTime's or a chest strap at rest and walking, and read the diagnostics'
  heart rate counters if the screen shows nothing.
- [ ] **Heart rate for apps that are not open.** A closed app's `heart_rate` subscription gets the
  latest reading the interval setting took at each of its periods, but it never starts a
  measurement itself, and no log of the readings is kept beside the step log.
- [ ] **Try the beacon on the watch.** The driver's register writes match wasp-os's, but the
  emulator cannot show the LED.
- [ ] **Run `tools/check_heart_rate.py`, `tools/check_call.py`, `tools/check_wake_gestures.py` and `tools/check_firmware_trial.py` in CI.** The emulator suites have no
  compiler to build the apps they install; the watch images job could build the install into `dist/` for them.

### UI toolkit

- [ ] **Double line buffers with DMA,** so one is filled while the other is sent. InfiniTime uses
  two of 240 x 4 pixels. This also fixes the text edges listed above.
- [ ] **More fonts and pictures** loaded from the external flash, beyond the shared library's. A
  font of lines and arcs is about 2 KB, so another can be a file in `/resources/` read into the
  open app's heap. Check one as app images are checked, keeping the limit of 8 shapes a glyph so a
  bad font cannot slow drawing enough to trip the watchdog.
- [ ] **Accented letters for notifications.** The text font holds ASCII and seven signs, so é, ü
  and the like draw nothing. Let a glyph in `app-host/fonts/geometric.txt` be a letter and an
  accent placed over it, and add the Latin-1 letters.
- [ ] **The browser preview's text font.** `sdk/neotime/src/widgets.tsx` draws text in JetBrains
  Mono, which the watch no longer uses. Draw the font from lines and arcs on the preview's canvas,
  so its letters match the watch.
- [ ] **Drag to change an Arc.** The app host gives `Arc` no `onChange`, so the metronome's tempo
  ring cannot be dragged; only tap tempo works.
- [ ] **A canvas widget for free drawing.** Paint fills cells of a 20 by 20 grid; InfiniTime
  paints where the finger is.
- [ ] **A Tabs widget** that swipes between pages under a row of tabs. It is laid out as a plain
  column now; `Pages` does the swiping without tabs.
- [ ] **Cyrillic text.** InfiniTime's 20 px font has Cyrillic letters for notifications and music
  titles. Add them to `app-host/fonts/geometric.txt`.
- [ ] **More library icons:** brightness levels, sleep, notifications on and off, list,
  home, eye, ban, phone off, volume mute, tachometer, paintbrush, paddle, map, shield, and the
  moon and night rain weather icons. Quick settings and the ports need them.
- [ ] **UI design guidelines.** Add InfiniTime's rules to `app-reference.md`: buttons at least 50
  pixels high and on the bottom edge, room for a page indicator and top bar, switches twice as
  wide as tall, and black backgrounds.

### Apps and faces

- [ ] **Try the system app on the watch.** It has run only in the harness and the simulator.
  Flash a firmware whose library carries its `info` icon first, then check the version,
  the uptime after the phone sets the clock, the reset reason after a button-hold reset, and the
  free storage before and after an install. On the About pages, check the Bluetooth address
  against the computer's, the flash, touch and accelerometer chips (a BMA425 here), that the
  touch panel's three ID registers read in one go, and that the task table's shares add up to
  something sensible. The bootloader page shows `CRC` until a bootloader with the version record
  is sent with the bootloader updater.
- [ ] **The display's state on the system app's pages,** as InfiniTime's System Info screen shows.
- [ ] **The full set of navigation turns.** Phones send about 50 icon names, such as forks,
  merges, ramps, ferries and roundabout exits, and the distance as text. NeoTime has 10 turns and
  always prints metres. Add InfiniTime's navigation pictures, map each name to one, and show the
  distance as the phone writes it.
- [ ] **Night icons in the weather app.** It shows the sun at night where InfiniTime shows the
  moon. One shared night-aware lookup would also replace the copies in the faces.
- [ ] **A 40 pixel seven-segment font,** so the Casio face's day-of-year and date lines match
  InfiniTime.
- [ ] **Bebas Neue and Teko for Infineat,** which draws its time and date in them; NeoTime
  substitutes its own font of lines and arcs.
- [ ] **The rest of the music values.** InfiniTime's music service also carries the album, the track
  number and total, playback speed, repeat and shuffle, and moves the position on at the playback
  speed while a track plays. The runtime has no value for any of them.
- [ ] **Music disc artwork.** InfiniTime's music app shows a disc that spins while a track plays.

### Simulator

- [ ] **Simulated parts** for touch, the accelerometer, the battery and Bluetooth, plus
  notifications once those exist.
- [ ] **GIF capture,** as InfiniSim has.

### External flash and storage

These live on the `worktree-bootloader`, `worktree-flash-async` and `worktree-updates` branches.

- [ ] **Shrink the BLEFS service.** It grew from 1.9 KB to 8.9 KB of flash when it moved onto the
  async store, and the full firmware and its library have about 8 KB of the 480 KB region left. Look at the copy, move and
  listing code and at how many async state machines its `answer` builds.
- [ ] **Try the flash store on the watch.** It runs in the emulator, apps and icons included, but
  has not met the real chip. Measure the mount time (about a second estimated), the deepest stack
  with apps open and saving, and read the diagnostics' store counters after a day of use.
- [ ] **The step log in the store's log ring.** Each day is a record rewritten whole at every slot,
  and the oldest days are deleted past 90; the log ring appends each slot alone and drops the oldest
  days by itself. Heart rate and activity logs belong beside it, and raw accelerometer recordings in
  a ring of their own. Until the clock is set after a restart, steps are logged under the
  firmware's build date.
- [ ] **A checkpoint of the store's tables,** so mounting need not read every ring record.

- [ ] **Use the file system:** save alarms and the BLE bond there.
- [ ] **Resource packages:** fonts and pictures installed from a zip over BLE, with a list of
  obsolete files that an upgrade deletes, as InfiniTime's `resources.json` has.
- [ ] **Report failed flash writes.** After each erase and page program, read the flash's security
  register and return an error when its erase-fail or program-fail bit is set, so the store refuses
  the write instead of trusting it.
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
- [ ] **Update progress as a bar.** The status page shows the kilobytes, the image size and a
  percentage in words. Draw a bar under them as well, as InfiniTime does.
- [ ] **Give up on a stalled update.** InfiniTime aborts an update when no packet has arrived for
  10 seconds; NeoTime only aborts when the link drops.
- [ ] **A swap progress bar on the bootloader screen.** It says "Installing firmware" for up to 30
  seconds with nothing moving. Its 20 KB has about 2.7 KB to spare.

### Developer tools and docs

- [ ] **A flaky app-host test.** `an_app_list_in_columns_lays_its_apps_out_as_square_cells` failed
  once in `app-host/src/fonts.rs` and passed on every rerun. The installed font is global, and
  `GeometricFont::bytes` reads its start and length separately while parallel tests install it,
  so a read can pair one install's start with an empty library's length. Read both in one step, or
  give each test its own font.
- [ ] **Measure the stack with any app open.** `tools/measure_stack.py --open-first-app` opens
  the launcher's first row, so with only the heart app installed it measures that: 7,080 B, 1,608 B
  spare, under the 2 KB the rest keep. The alarm's 10.5 KB peak and the system app's still cannot
  be measured again. Let it open a named app, and shrink the heart app's peak.
- [ ] **Clippy fails with warnings denied.** `cargo clippy --release -- -D warnings` stops on two
  `needless_borrow`s in `firmware/src/main.rs` (`with_attribute_server(&server)`) and three in
  `neotime-runtime`.
- [ ] **No MPU in pinetime-engine.** The engine models no memory protection unit, so the
  stack-guard test skips there.
- [ ] **A DFU upload tool in `tools/`** that sends a zip over BlueZ, as wasp-os's `dfu.py` does,
  including type `0x08` for the recovery slot, which no client sends yet.
- [ ] **Flashing a watch over SWD.** One image with the bootloader and the firmware, a way to fill
  the recovery slot on a watch flashed this way, and a page on probes, the SWD pins and RTT logs.
- [ ] **Releases.** A version scheme, and a page saying what each firmware file in a release is
  for; `.github/workflows/release.yml` already publishes `dist/` and the store for each `v*` tag.
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
| Clock | the Current Time client |
| Notifications | the Alert Notification client, Immediate Alert |
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
- [ ] **Widgets:** status bar, battery meter, clock, notification bar, button,
  toggle, checkbox, slider, spinner, stopwatch and confirmation view.
- [ ] **App icons:** monochrome Font Awesome icons per app.
- [ ] **The rest of InfiniTime's grid launcher:** remember the last page, and grey out apps and
  faces whose resources are missing. Lists and grids, chosen in the companion, are done.

### Simulator

- [ ] **Run every app and face** in the simulator.
- [ ] **A smoke test** that opens every app in the simulator and checks its screenshot.

### External flash and storage

- [ ] **Install apps as packages over BLE,** as the fork's `pkgmgr` does into `/flash/pkg`.
- [ ] **File manager and backup** from the companion app.
- [ ] **Updatable read-only image:** the fork keeps nearly all its code in a ROMFS image in
  internal flash, updated without reflashing the firmware.

### Firmware updates

- [ ] **Firmware from GitHub releases** offered by the companion app.

### Not started

The fork has these and NeoTime has nothing for them yet. Most also appear in the InfiniTime table.

| Area | What the fork has |
|---|---|
| App runtime | apps loaded when opened, state kept when left |
| Heart rate | raw data logging |
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

- [ ] **Quick face switching.** PinetimeLite moves to the next face on a long press of any face.
  Decide whether NeoTime has a gesture for this, since several faces use the long press for their
  own settings.

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
