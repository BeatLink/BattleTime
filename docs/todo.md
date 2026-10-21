# To do

Work that is known to be needed but not done yet.

When an item is done, remove it here. If it is something the user sees, add it to the matching
page of the [guide](guide/index.md); if not, describe it in [architecture.md](architecture.md) or the
document for its part instead.

## Hardware tests

- [ ] **Put a changed system file again even at the same size.** The companion compares each system
  file by its size alone, so a new verifier module of the same length as the old one would stay
  unsent, and the watch would check apps by the older rules. Compare a checksum, such as a module
  file's CRC in its header, or the store's own.

- [ ] **The SOS app on the PineTime.** The `sos` app has run only in the simulator and harness.
  Check that holding the button for five seconds keeps reporting the finger until it lifts, so the
  hold completes and an early release cancels; that a swipe to the Signals page while holding
  cancels the hold; that the light, screen and vibration flash "... --- ..." in step; and that an SOS
  stopped from the phone, by its `{"on":0}` message, takes the open app back to its hold button. The phone's half is in
  [the companion's TODO](../companion/TODO.md#sos).

- [ ] **A message from the phone run in the background on the PineTime.** Start an SOS, leave the app,
  send `{"t":"app","ch":"sos","d":{"on":0}}` and check the SOS app opens at its hold button with no `on` 0 sent back.

- [ ] **A firmware upload through `SlotFlash` on the PineTime.** The update and recovery slots are
  now erased and written through `SlotFlash`, which waits on the timer instead of spinning. Moving
  the recovery slot over the Nordic UART is proven in the emulator (`tools/check_slot_move.py`),
  but no emulator check uploads a firmware by DFU. Flash an update with `battletime flash` and check
  that it installs and confirms, that the screen and touch keep answering while it arrives, and that
  `battletime slots` still answers afterwards.

- [ ] **The DCDC regulator on the PineTime.** The firmware turns the nRF52832's DCDC regulator on
  at start (`config.dcdc.reg1`), which has run only in the emulator, which models no regulator.
  Flash it and check that the watch boots, connects, and that the battery percentage in the
  About app and `battletime battery` reads about the same as before and falls no faster over a day.

- [ ] **TWIM power-down on the PineTime.** `i2c_bus::SleepingTwim` powers TWIM1 down after every
  transfer for nRF52832 anomaly 89, which the emulator does not model. Check that touch, steps and
  a heart rate measurement still work, then compare the battery's fall over a night, screen dark,
  with the build before it; the anomaly costs about 400 µA, roughly a quarter of the idle drain.

- [ ] **Background heart rate waiting for movement on the PineTime.** Background measurements now
  give up after 15 s, and after one gives up the next waits for new steps or movement. With the
  interval at every minute, check that readings still come while worn, and that with the watch
  lying on a table the sensor stops flashing after the first try and starts again once it is
  picked up.

- [ ] **Raise and shake at 5 Hz on the PineTime.** With the screen dark the motion task now reads
  the accelerometer five times a second instead of ten, as only raise and shake are watched then.
  Check that raising the wrist and a shake at the usual **Shake strength** still wake the screen
  about as reliably as before, and recalibrate the strength if shakes are missed.

- [ ] **DISPLAY_OFF/ON on the PineTime.** The display driver sends DISPLAY_OFF before sleeping the
  panel and DISPLAY_ON after waking it. Check that the screen turns off and on cleanly with no
  flash of old content.

- [ ] **The face chooser on the PineTime.** Settings' **Face** row opens the shown face full
  screen, where swipes left and right change it; it has run only in the harness and the emulator,
  which showed the chooser opening, a swipe choosing the next face and the button going back to the
  app. On the watch, check that each face slides in the swipe's way with the time live, a tap or
  long press on a face such as PineTimeStyle changes nothing, and the button comes back to Settings
  showing the new face's name, which the face then keeps after a restart.

- [ ] **Paging on the PineTime.** `Pages` (2f9524a) and the system and steps apps built on it
  (5957275) have run only in the harness and the emulator; the watch still runs 3a5abe9. Flash a
  build with them and check on the watch that a swipe left or right wipes to the next page and
  never skips one, even a long slow swipe, which the panel repeats as it goes, the
  page bar marks the page on show, a sideways drag leaves the system page's list still, swipes past
  the first and last page step round the quick apps, and a charger event opens the battery page.
  With one press and one swipe per finger (71dc8077), swiping slowly and long through the System
  app turns exactly one page each time, System to Licence and back, and a tap and a vertical
  scroll on the Tasks page still work.

- [ ] **Launcher freeze fix on the PineTime.** On e266858f the watch froze on the face when the
  launcher opened, with the button still working, and the watchdog reset it with no crash. The
  display and flash-read waits could sleep in WFE after the bus had already been freed, and
  cad2538f makes the bus interrupt send SEV every time. With every app installed, open and close
  the launcher about ten times and open a few apps; expect no freeze and no "Last restart:
  watchdog" in `battletime debug`.

- [ ] **DMA flash reads and prefetch on the PineTime.** Reading the external flash by DMA, with
  `file_bytes` offsets and `prefetch_file` (9d4987d7), has run only in the emulator, which models no
  nRF52832 DMA anomalies. Install a throwaway `pager` app whose tap reads 256 bytes of
  `/apps/pager/data` at `page * 1900` and prefetches the next page, labelled
  `${page} ${bytes[0]} ${bytes[128]} ${bytes[255]}`, and a 12,000-byte data file where byte i is
  `(i * 7) % 251`. Read the diagnostics, tap three times and read them again. Expect exactly
  "1 0 143 28", "2 248 140 25" and "3 245 137 22" (page 3 crosses a 4 KB sector), PrefetchedReads
  up by 16, FlashReadBytes up, and the face, launcher and an ordinary app still drawing normally.
  A wrong byte means a DMA read corrupted data. Remove the app and its file afterwards.

- [ ] **Resuming a firmware update on the PineTime.** The watch keeping what reached flash and the
  companion reconnecting up to four times to send the rest (9c9cb262) has run only in the emulator.
  An ordinary `battletime flash` finishes and confirms. Then flash the same zip again and, at 20-30%,
  run `bluetoothctl disconnect FC:9B:D8:FD:4A:F8`. Expect "Update paused" and "Kept N KB" on the
  watch, "Lost the watch; reconnecting to carry on (try 1 of 4)..." from the CLI, its progress bar
  starting again near the kept amount rather than 0%, and the update validating, installing and
  confirming, with the new build on the About app's firmware page. Once, the watch was seen
  pausing at "Kept 3 KB" each time with the transfer starting from scratch, so check for that.

- [ ] **Text drawing time on the PineTime.** The geometric font's square roots in one instruction
  (5b035fed), text advances without flash reads (4af364ab) and the flash instruction cache
  (d1e86190) have not been timed on the watch. Leave it untouched on the digital face and read the
  diagnostics characteristic `6e540002-0000-4000-8000-4e656f54696d` raw: words 22 and 23 (little
  endian) are DrawMicroseconds and DisplaySendMicroseconds. Send `battletime notify --sender Messages
  "Dinner plans" "Running ten minutes late. Save me a seat by the window, and order the usual if
  they ask first."`, wait 12 s and read again, three times, then once more after 40 s of quiet.
  Earlier draw times were 419 ms with bitmap fonts and 379-749 ms with the geometric font; expect
  less, ideally near 419 ms.

- [ ] **Flash cache hits and misses on the PineTime.** The diagnostics count the instruction cache's
  hits and misses (8b25f803), to calibrate the emulator's cache model. With every app installed and
  the face showing, read the raw diagnostics characteristic: words 31 and 32 (little endian,
  counting from 0) are FlashCacheHits and FlashCacheMisses since start. Read again after 60 s
  untouched, after swiping up to the launcher and waiting 3 s, and after tapping the first app row
  and waiting 4 s. The emulator misses 10.4% idle on the face, 4.7% opening the launcher and 5.0%
  opening Settings; the watch should be within about a point of each, or the emulator's assumed
  cache line size needs changing.

- [ ] **Terminal face on the PineTime.** With every label in the small font (51e32c64), all ten
  lines fit on the screen: "[1]+ Notify" when a notification is unread, "user@watch:~ $ now", the
  [TIME] [DATE] [BATT] [STEP] [L_HR] [WTHR] [STAT] lines and the final "user@watch:~ $" prompt
  fully visible at the bottom. Note whether the smaller text is still readable.

- [ ] **The state message on the PineTime.** The firmware serving the companion's
  `{"t":"state","app":"<id>"}` message (b06d9b39) has run only in tests. Set one alarm at 07:00
  and leave the Alarm app open. Rewrite `/apps/alarm/state` with the hour changed to 08, as the
  companion core's `sync::pass::write_app_state` does, and send `GB({"app":"alarm","t":"state"})`.
  Expect the open app to redraw in place at 08:00 with no reset, 08:00 again after reopening it,
  and the alarm ringing at 08:00. Repeat with the app closed; it rings at the new time. Then hold
  it with `GB({"app":"alarm","hold":true,"t":"state"})` a minute before a one-time alarm: expect
  no ring until the plain `state` message follows, or until 30 seconds pass.

- [ ] **The companion app store with the PineTime.** The store (51d5de81, b0909f02) has not
  installed anything on a real watch; no firmware change is needed. From `companion/`:
  - `battletime store list` lists 54 apps and faces, alarm to word-clock, leaving out the
    companion-only prompt app;
  - `battletime apps install dice` installs the bundled dice app by id, printing "Installed Dice as
    dice.", and dice shows in `battletime apps list`;
  - `battletime store fetch BeatLink/BattleTime 1 <folder>` prints "Fetched 93 from v0.1.0 into
    <folder>." (56 apps and faces, 37 plugins);
  - installing an app from that folder that is not on the watch, such as `haiku.ntapp`, prints
    "Installed Haiku as haiku." and it appears in the launcher;
  - `battletime apps remove` takes it, and dice if it was not there before, out of `apps list` and the
    launcher;
  - in the Tauri desktop app, Watch tab, Store, Look on GitHub, v0.1.0, adding and installing Buzz
    back puts the plugin under Plugins and a Buzz back app on the watch, and Uninstall removes both;
  - in the same app, opening the Store with no GitHub step lists Buzz back under Plugins, from the
    newest release, and Install downloads it and puts it and its watch app in place.

- [ ] **Pomodoro on the PineTime.** The pomodoro rebuilt around three intervals (4d27f48a) has run
  only in the harness and the browser preview. Check on the watch that it opens on the countdown,
  that the three length pages are reached by swiping right and their - and + buttons set the
  lengths, and that a round runs work and short breaks in turn with a long break after the set
  number of work sessions, buzzing and waking the screen between each.
- [ ] **Files on the PineTime.** The Files app and its `open_folder` listing (960ce130) have run
  only in the harness and the browser preview. Check on the watch that it opens at `/` with each
  folder's size and no stall or watchdog reset, that tapping `apps` and then an app's folder lists
  its files with byte sizes, and that each side button press goes up one folder, wiping the
  opposite way, until one more press at `/` leaves the app. Afterwards Settings > System > memory
  shows nothing unusual.

- [ ] **Touch panel on the PineTime.** The firmware runs `battletime-touch` on the watch, but these
  checks have not been made:
  - `start` succeeds and `read_chip_info` gives a sensible chip ID (0xb4 or a near value);
  - with the 30 px swipe distance, short upward swipes count and a scroll in a list never turns
    into a swipe; taps, long presses and longer swipes were already right on the watch (touch test,
    2026-10-14);
  - no tap follows a long press any more, now the panel's late report of a lift is ignored;
  - a flicked list glides on and stops naturally, a slow drag lifted gently does not glide, and a
    tap on a gliding list stops it without opening the row;
  - a tap that wakes the screen opens nothing;
  - a computer's link holds for a whole session once the link's pace has asked for its 6 s supervision
    timeout (`btmon` shows the central's connection update);
  - the corners and edges report positions near 0 and 239, or edge swipes need a margin;
  - positions match the display, with column 0 and row 0 at the top left;
  - `sleep` stops the interrupts, and `sleep_until_double_tap` wakes on a double tap only;
  - `wake` brings back the settings `start` was given.

- [ ] **Accelerometer on the PineTime.** The firmware counts steps with
  `battletime-accelerometer` on the watch, but these checks have not been made:
  - lying flat with the screen up reads about +1000 milli-g on z, and tilting it reads the
    expected signs on x and y, which settles whether `Orientation::PINETIME` is right;
  - walking raises `read_step_count`, and `reset_step_count` sets it back to 0;
  - the upload still works with chunks larger than 16 bytes, which would make starting faster.
  - raise to wake, lower to sleep and shake to wake act on a real wrist as in the emulator, the
    default shake strength of 150 is neither too eager nor too hard, and reading ten times a
    second while the screen is dark costs little battery.

- [ ] **Step history on the PineTime.** It has run only in host tests and the test watch. On the
  watch, check that:
  - walking fills today's hours, and a block appears in `/day_blocks/steps` within six minutes;
  - a restart keeps the day, the week and the trip count, losing at most six minutes;
  - local midnight moves today to yesterday with the steps app closed;
  - the phone setting the clock after a restart reads the right week back;
  - the slot writes every six minutes do not upset the Bluetooth link.

- [ ] **Heart rate and battery logs on the PineTime.** They have run only in host tests. On the
  watch, check that a block appears in `/day_blocks/battery` within seven minutes of a start, that
  `/day_blocks/heart` fills while the Heart app or the background interval measures, that the
  charging bit follows the charger, and that the companion's Heart and System pages show the days.

- [ ] **Bluetooth controller on the PineTime.** The firmware runs `battletime-bluetooth` on the watch and
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
    so the 1 ms allowed for the 64 MHz crystal to start is enough on this watch's crystal; time
    `EVENTS_HFCLKSTARTED` against the crystal start task to confirm it, as it typically takes 0.4 ms;
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
  - on a firmware sent and not yet confirmed, the About app's Confirm keeps it and Roll back
    restarts into the one before;
  - a preset theme recolours the open app and the next one opened;
  - settings survive a restart, from `/settings/system` and `/settings/theme`;
  - a `settings` message from the companion app changes them and the reply lists every key.
- [ ] **Files over BLEFS on the PineTime.** The file service has run only in the emulator, against a
  raw HCI client. On the watch, with the companion's `battletime files` commands, check that:
  - the phone reaches an MTU of 124, so a write of up to 121 bytes is taken whole;
  - listing, making a folder, writing a file, reading it back, moving it and deleting a folder work;
  - a long listing arrives complete, with no notification lost while packets are short;
  - free space reads back through the companion's scratch-file write.
- [ ] **Installing apps on the PineTime.** On the watch, firmware ae36259 took the shared library,
  the launcher, a face and every app through `battletime apps install` and listed them with `apps`.
  Still check on the watch that:
  - the apps appear in the launcher without a restart;
  - hiding, showing and removing one updates the launcher;
  - an image with a bad id or a library that fails its check is refused with an error.
- [ ] **The Software app and memory checks on the PineTime.** They have run only in the emulator.
  On the watch, check that the Software app lists every app with its memory, that its switches turn
  apps off and on, that the launcher and the last face refuse to turn off, and that `battletime apps
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
  after the restart, that Restart and the side button go on to the face, and that `battletime debug`
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
  - the stack keeps its margin with the app open, which in the emulator matches the launcher's;
  - with no note (`battletime note remove`), the app shows "No note yet. Write or choose one in the
    BattleTime companion app." over four lines, as in
    `battletime/harness/snapshots/screens/app-notes-empty.png` (e266858f).
- [ ] **The remote app on the PineTime.** It has run only in the test harness. With the companion
  app's remote screen, check that:
  - buttons chosen in the companion app reach `/remote/buttons` and show the next time the app
    opens or the screen turns on;
  - each tap's `{"t":"app","ch":"remote",...}` line reaches the phone, which runs that button;
  - the stack keeps its margin while a tap's line is sent.
- [ ] **The QR code app on the PineTime.** It (9e8f7b13) has run only in the test harness and the
  companion's mock watch. With the companion app's QR code screen or `battletime qr`, check that:
  - `/qr/list` and the codes in `/qr/<slot>` arrive, the list shows them the next time the app
    opens, and a tapped code is read and drawn without a long pause;
  - another phone scans a short link, a Wi-Fi code and a code over 74 modules;
  - drawing the largest code stays quick enough for the watchdog.
  - with up to eight codes (87dff11f, which needs the qr-code app installed from the same commit):
    `battletime qr https://example.com --label Example` and `battletime qr wifi Home secret` print
    "Added ... as number 1" and "2", and `battletime qr list` prints "1. Example" and "2. Home";
  - the app lists "Example" and "Home" with the screen bright, and a tapped row draws its code
    black on white with its caption under it, without a long pause or watchdog reset;
  - after `battletime qr move 2 up`, `battletime qr rename 1 "Guest Wi-Fi"` and `battletime qr remove 2`,
    reopening shows only "Guest Wi-Fi";
  - after `battletime qr remove all`, it shows "No QR codes yet. Make them in the BattleTime companion
    app."
  When these pass, also remove the QR item under "Untried with a watch" in companion/TODO.md.
- [ ] **The survival app on the PineTime.** It has run only in the test harness and the
  companion's tests. With the companion app's Survival screen or `battletime survival`, check that:
  - `battletime survival set cpr bleeding lost` writes `/survival/list` and the three guides, and the
    app lists the topics "First aid" and "Wilderness" the next time it opens, "First aid" lists "CPR"
    and "Severe bleeding", and the side button goes back through the topic to the topics;
  - with `battletime survival set all`, "Personal safety" lists its last guides, which come from the
    list's third part;
  - a tapped guide shows its title and text, and the longest guide scrolls smoothly to its end;
  - `battletime survival remove lost` takes "Lost in the wild" off the list and deletes `/survival/9`;
  - after `battletime survival remove all`, it shows "No guides yet. Choose them in the BattleTime
    companion app."
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
  `dist/bootloader-updater.zip` with `battletime flash`, then on the watch check that:
  - off the charger the updater says "Plug in the charger" and its line reads "unplugged";
  - on the charger the line reads "charging" with a charge close to the About app's, and the
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
- [ ] **The phone's own services on the PineTime.** The client for the phone's Current Time and
  Alert Notification services has run only against `tools/check_phone_services.py` playing the
  phone through the emulator's bridge, and against the pretend phone in `battletime-phone-client`'s
  tests. On the watch, with an Android phone serving both through nRF Connect's GATT server, check
  that a few seconds after connecting the clock takes the phone's time and zone, the firmware is
  confirmed after an update, a New Alert notified by the phone shows as a notification, and that
  Gadgetbridge, the companion app and a phone serving neither service work as before.

## Bluetooth controller

See [bluetooth.md](bluetooth.md) for the details of each.

- [ ] **Encryption and pairing.** Answer `LL_ENC_REQ` and encrypt packets with the chip's CCM and
  ECB peripherals, so phones that insist on pairing can connect.
- [ ] **Peripheral latency on the watch.** The link layer skips the events the central's latency
  allows, tested only on the computer. On the watch, with a phone that grants latency, check that
  the link holds, that notifications still arrive promptly, and the current drawn while idle.
- [ ] **The link's pace on the watch.** The watch asks for slow parameters while idle and fast ones
  during transfers ([bluetooth.md](bluetooth.md#the-links-pace)), tested only on the computer. On the
  watch, with `btmon` on a computer or a phone, check that the phone accepts the slow parameters, and
  a file transfer runs at full speed.
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

See [updates.md](updates.md). The reloader moved a watch from wasp-os onto BattleTime, and an update
over Bluetooth and the button-hold recovery have both worked on that watch.

- [ ] **Check why wasp-os's update mode gives way to the reloader** about 4 seconds after the
  button escape in the emulator, with no reset between. Its advertising never times out, so this may
  be the emulator's SoftDevice; if a watch does the same, the escape leaves the reloader running.
- [ ] **Run the DFU service against Gadgetbridge:** wasp-os's `dfu.py` and the companion app
  already update the watch.
- [ ] **An update over Bluetooth in the emulator.** The update suites write images straight into
  flash, so no emulator test sends a firmware through the DFU service. Send one over the
  PineTimeEmulator Bluetooth bridge from a raw-HCI client, which also proves the controller's sleep
  between events holds through a long transfer.
- [ ] **The shell's store calls without blocking.** The storage task lets the CPU go while the flash
  works, but the shell calls the store directly and polls the chip, so a write it makes, such as an
  app's saved state, blocks every task for up to a sector erase. Make the shell's loop await its
  store calls, and the app host's `read_file`, which `TextFile` and `file_bytes` use while drawing,
  `async`; the open path's recursion (`open` and `open_failed`) needs reworking first.
- [ ] **Pause an erase for a read.** The XT25F32B suspends an erase in about 94 µs (0x75, resumed
  with 0x7A, measured in [external-flash-architecture.md](external-flash-architecture.md)), so a
  read that finds the chip erasing could pause it instead of waiting up to 50 ms. The driver has
  `suspend_operation`, `resume_operation` and `read_status_registers`; the store and `display_dma`
  still wait.
- [ ] **Signed updates**, through embassy-boot's ed25519 check in `UpdateReceiver::finish`.
- [ ] **Choose what runs over Bluetooth.** Rolling back to the previous firmware or installing the
  recovery firmware needs the bootloader's button menu. A companion command such as
  `battletime boot previous|recovery` could do the same from the firmware: check the image's header and
  CRC, copy it into the update slot (`copy_previous_into_update_slot`, `copy_image_into_update_slot`),
  mark a swap and restart, so it runs on trial like any update.
- [ ] **Find why the first `slots` after a flash timed out.** On 2026-10-13 the first
  `battletime slots`, sent right after `battletime flash` and `battletime time`, got no reply; every later
  request answered at once and it did not happen again.
- [ ] **Install recovery from a resized slot on the watch.** After `battletime slots fit` shrank the
  watch's recovery slot to 168 KB, the firmware read the record back, but the bootloader menu's
  Install recovery from that slot has only run in the emulator; it needs a button hold.
- [ ] **Run the stack overflow test in the emulator.** `crash_record`'s "A Stack Overflow Faults At
  The Guard And Is Reported" skips, since PineTimeEmulator has no memory protection unit yet; it
  runs once the emulator models one.
- [ ] **Read the bootloader in the updater with assembly loads.** `bootloader-updater`'s
  `installed_crc` reads `BOOTLOADER.start` (address 0, the null pointer) through `Nvmc`, which is
  undefined behaviour; the firmware's same read let the compiler delete most of the firmware. Use
  loads like `status::BootloaderPages` in the firmware.
- [ ] **Gadgetbridge with the InfiniTime name.** The `gadgetbridge_name` setting makes the watch
  advertise as "InfiniTime" so Gadgetbridge's PineTime/InfiniTime coordinator takes it, but no real
  Gadgetbridge has paired with it yet. Try it with InfiniTime's Simple Weather and Immediate Alert
  services, and with a build that has the other three, and check whether Gadgetbridge insists on bonding, which
  the controller cannot do yet. A coordinator of BattleTime's own in Gadgetbridge would need no setting.

## UI toolkit

- [ ] **Measure the UI heap on the watch.** It is 20.25 KB, shared with the system's 10 KB; so
  far it is measured only in the emulator, where the alarm's editor touched 13,188 B of the 31,744 B
  the heap had then. Count what a typical screen allocates on
  the watch, then update the "UI heap" row in
  [memory-map.md](memory-map.md) and the size in `tools/draw_memory_maps.py`, and rerun the script.
- [ ] **App screens in the simulator.** The simulator still shows hand-drawn demo pages. Draw
  real app screens there through `ScreenRenderer`.
## App runtime

- [ ] **Page strings for an app with a lot of text.** Paged apps keep their strings in RAM. Paging
  them was built (branch `worktree-string-paging`) and left out because it costs more than today's
  apps save; cut its code and stack cost and group each screen's strings, as
  [external-flash-architecture.md](external-flash-architecture.md#paging-strings) describes, once an
  app's strings reach several kilobytes.
- [ ] **Profiles.** Sets of apps such as work, gaming or comms, picked from the companion app. A
  profile is a small file in external flash listing app ids, their order, the watch face and which
  subscriptions are on. Switching profiles only reads files, so it writes nothing to flash.
- [ ] **Temporary apps the companion sends by itself.** The watch runs temporary apps from `/temp`
  and the companion can send one by hand (`battletime apps run`, `battletime ask`), but nothing sends a
  custom notification page or a game score when a notification or a score arrives on the phone, and
  the companion app has no screen for choosing them.
- [ ] **Try temporary apps on the watch.** They have run only in the emulator. On the watch, check
  that `battletime ask` opens the prompt over the face and over another app with the screen dark and
  lit, that Yes and No reach the computer and go back, that a second `battletime ask` reuses the cached
  image, and that turning Bluetooth off on the computer closes the prompt.
- [ ] **More phone data as inputs.** Weather, music, navigation and calls already reach apps as
  system values; others, such as the next calendar event, unread count or a game score, could
  join them, so many apps are only a layout bound to phone data.
- [ ] **Internal flash writes while Bluetooth runs.** Erasing an internal flash page stops the CPU
  for tens of milliseconds, so the radio can miss its connection events. Updating the shared
  library or filling the internal-flash cache while the watch runs needs erases timed between
  connection events, or Bluetooth paused for the write.
- [ ] **Resource cache for external flash.** A small fixed area of RAM with two or three slots,
  reused least recently used first. Each slot is filled by one DMA read of a whole resource, such
  as a font or picture from a resource package or a handler from `/apps`, so each costs one SPI
  transaction instead of many small reads.
- [ ] **The remaining companion messages.** [companion-protocol.md](companion-protocol.md) lists
  them under "not served yet". `call`, `musicinfo`, `musicstate`, `weather`, `nav`, `find`, `vibrate`,
  `status`, the phone's `app` messages for one watch app (`services/companion/src/app_message.rs`) and
  the `music`, `call`, `findPhone`, `request` and `app` lines are served (`services/companion/src/phone.rs`).
  - `alarm` (set or report the alarms), `steps` (a day in 240 six-minute slots, read from the
    step log with `battletime_steps::log`) and `http` (answers to an app's request, and the watch's `http`
    line).
  - The weather's coming days, sunrise and sunset: Gadgetbridge's `weather` line carries none, so
    only InfiniTime's Simple Weather service fills them; the companion app needs keys of its own.
  - Navigation progress: Gadgetbridge's `nav` carries none, so `progress_percent` reads 0 unless
    InfiniTime's navigation service, not yet in the shipped firmware, writes it.
  - Gadgetbridge prefixes each line with the `0x10` byte, which only the `setTime` line reader
    skips; check that `GB(...)` lines arriving with it are read, and skip it in `parse_line` if not.
  - Keep the music and weather through a restart; they are kept in RAM only.
  - Nothing has run on the watch yet: try each message from Gadgetbridge and the companion app.
- [ ] **Harness gaps in the app host.** The test harness works around three: `widget_bounds` has
  no clip rectangle, so it cannot tell a widget scrolled out of a List or Scroll; there is no
  iterator over the open screen's nodes; and there is no public "redraw everything" (it re-applies
  `set_theme`).
- [ ] **Read handlers from the file when they run.** The shell reads an app's whole image into the
  UI heap when it opens, so every byte of it costs RAM. Keep only the expressions and the functions
  they call, which run on every redraw, in RAM and read a handler from `/apps` when its event
  comes. Apps stay on the external flash; nothing of an
  app is copied into the internal flash.
- [ ] **Redraw periods per branch.** `analyse_expression` in `battletime/runtime/src/bindings.rs`
  takes the finest step anywhere in an expression, so `on ? now.value : start_of_minute(now.value)`
  redraws once a minute even when the raw branch shows. Treat a clock read that reaches the result
  uncut as one second. Checking only the next instruction makes digital and casio wake each second,
  because `minute_of` and `day_of_year` go through locals.

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
  what a `battletime/lib` function does should call it. One-line helpers such as `(x + 1) % n` stay
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

- [ ] **Check the free heap before reading an image beside an open app.** A packages `install`, the
  launcher reading icons and opening an app all read a whole image while another app may hold the
  heap. The package manager now refuses images larger than an app's budget, but one inside it can
  still meet a heap the open app has mostly taken; read `LlffHeap::free` first and refuse instead.
- [ ] **Packages in the simulator.** The simulator gives no `packages` values, so the Software app
  shows an empty list there; feed it the apps it loaded, through `battletime-packages`'s checks.
- [ ] **Count the heap's peak exactly.** The emulator measures the allocator's bytes in use and
  the highest heap byte touched, which misses allocations filled with zeros. The allocator's
  low-water mark (`restart::least_free_heap_bytes`, on the About app's memory page) is not read over
  the diagnostics characteristic yet; reading it there would check the heap estimate against every
  app's worst moment rather than its first screen.

## Display and SPI bus

- [ ] **A deeper descender for the small text size.** The firmware's own pages draw in the `small`
  size, whose descender gets about 3 pixels, so a g's tail looks squashed, as in the Checking
  page's "Navigation". Give the small size its own descender depth, or draw those pages one size up
  in wider cells.

- [ ] **12-bit colour mode.** The ST7789 can take 12 bits a pixel, which cuts the time to send a
  full screen by a quarter, from about 115 ms to 86 ms at 8 MHz. It costs banding in gradients and
  packing pixels in pairs. Measure it once real screens run on the watch.

## Gaps against InfiniTime

What InfiniTime has for each feature BattleTime already has, compared file by file with InfiniTime
`6c119eb5` on 2026-10-11. Items already listed in the sections above are not repeated. On the
`worktree-prune-superseded` branch of the BeatLink InfiniTime fork, the files BattleTime has fully
replaced are removed, so what is left there is the porting reference for these items.
The full report is [InfiniTime files against BattleTime](https://claude.ai/artifact/LqGLNFf5MvT5D39o44hfUv); the earlier feature summary is
[BattleTime vs InfiniTime: feature coverage](https://claude.ai/code/artifact/4a70bc3f-2b66-4bdf-8ba9-c9233c2f28ea).

### Board and system

- [ ] **Turn on the DC/DC regulator in the bootloader.** The firmware sets
  `config.dcdc.reg1 = true`, but the bootloader keeps embassy-nrf's default, the LDO. InfiniTime
  sets `DCDCEN` at start, which cuts the current the chip draws, most of all while the radio is on.
- [ ] **Recover the I2C bus.** InfiniTime clocks SCL 16 times at start to free a bus a sensor left
  held, runs the TWIM at about 390 kHz because of the nRF52832's 400 kHz timing erratum, and resets
  the TWIM when a transfer never finishes. BattleTime runs at 400 kHz with no time limit, so a frozen
  bus stops touch and steps for good.
- [ ] **Warn when a part fails at start.** InfiniTime shows a warning screen when the touch
  controller does not start; BattleTime shows nothing.

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
  hold each do what docs/guide/getting-around.md says, and a 5 second hold still resets into the bootloader.
- [ ] **Try the face's swipes on the watch:** down to the notifications and again to quick
  settings, up to put them away, down in the launcher, and left and right round the quick apps.
  They are tested on the host and in the harness only.

### Accelerometer

- [ ] **Room for InfiniTime's other services.** Music, Navigation and Motion are written, tested on
  the computer and in the emulator (`tools/check_infinitime_services.py`), but are cargo features
  left out of the shipped firmware for want of flash; together they need about 4.3 KB more
  ([bluetooth.md](bluetooth.md#infinitimes-own-services) has each one's cost).

### Bluetooth

- [ ] **Pairing with a passkey.** Secure Connections with a 6-digit passkey shown on screen, as
  InfiniTime does. Needs encryption in the controller first. The P-256 maths is ready as the `p256` native module
  ([external-flash-architecture.md](external-flash-architecture.md)), so pairing costs the firmware
  only the calls into it.
- [ ] **Bonding** saved to the file system and restored at start.
- [ ] **The Current Time from an iPhone.** iOS is expected to serve its Current Time service only
  over an encrypted link, so the watch's read of it is refused until pairing works; then an iPhone
  should set the clock with no app, as it does for InfiniTime. Check what an iPhone answers first.
- [ ] **Privacy:** resolvable private addresses.
- [ ] **Light the screen when a phone connects or leaves.** The shell redraws `is_phone_connected`
  and sends the `phone` event, but a dark screen stays dark; turn it on briefly, outside do not
  disturb, so the wearer sees the link change.
- [ ] **Advertising on the watch.** Fast advertising for 30 seconds, then about once a second, with
  InfiniTime's advertisement and the name in the scan response, has run only in the emulator
  (`tools/check_advertising.py`). Check on the watch that phones find it at both rates and that it
  keeps advertising, given **Bluetooth stopped advertising after an update** above.
- [ ] **A larger ATT MTU.** The firmware allows 124 bytes where InfiniTime asks for 256. File
  transfer, long track names and notifications need longer writes.

### App runtime

- [ ] **Pushed events for closed apps.** Closed apps run their timers, alarms, daily times, clock
  changes and heart rate periods, but steps, step goal, motion, battery, phone, notification, call,
  music, weather, navigation and settings-change subscriptions reach only the open app. Each would
  load its listening closed apps through `battletime_app_host::closed_apps` as the timers do.
- [ ] **An emulator suite for closed apps.** A timer ringing while closed was measured by hand in
  the emulator ([memory-map.md](memory-map.md)); no suite under `tests/emulator` repeats it, so a
  later change to that path is not checked for its stack and heap.
- [ ] **Deliver the remaining motion events to apps.** Of the `motion` kinds the shell pushes
  only shakes; push wrist raise, tap, double tap, still and moving.
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
  measurement itself.
- [ ] **Try the beacon on the watch.** The driver's register writes match wasp-os's, but the
  emulator cannot show the LED.
- [ ] **Run `tools/check_heart_rate.py`, `tools/check_call.py`, `tools/check_wake_gestures.py` and `tools/check_firmware_trial.py` in CI.** The emulator suites have no
  compiler to build the apps they install; the watch images job could build the install into `dist/` for them.

### UI toolkit

- [ ] **An app's own font,** for a face or app whose lettering the shared font cannot give, such as
  Infineat's below. Build it the first time one needs it. An app image can already carry a font:
  the compiler embeds the dot-matrix font as font 0 of each app that uses `dot_matrix`, and the host
  takes it from the image while the app is loaded (`fonts::install_app_font`). What is left is
  letting an app name a font of its own beside that one, and cutting each embedded font down to
  the characters the app can show: its fixed text, and the digits and signs of any value it
  formats. A face's digits and a few letters come to a few hundred bytes. Rejected: a font file in
  external flash, which would cost every app using it a whole font of heap, or else glyph reads on
  the SPI bus the display shares.
- [ ] **Drag to change an Arc.** The app host gives `Arc` no `onChange`, so the metronome's tempo
  ring cannot be dragged; only tap tempo works.
- [ ] **A canvas widget for free drawing.** Paint fills cells of a 20 by 20 grid; InfiniTime
  paints where the finger is.
- [ ] **A Tabs widget** that swipes between pages under a row of tabs. It is laid out as a plain
  column now; `Pages` does the swiping without tabs.
- [ ] **Cyrillic text.** InfiniTime's 20 px font has Cyrillic letters for notifications and music
  titles. Add them to `app-host/fonts/geometric.txt`.
- [ ] **More library icons:** brightness levels, sleep, list, home, eye, ban, phone off,
  volume mute, tachometer, paintbrush, paddle, map, shield, and the night rain weather icon.
  Quick settings and the ports need them.
- [ ] **UI design guidelines.** Add InfiniTime's rules to `app-reference.md`: buttons at least 50
  pixels high and on the bottom edge, room for a page indicator and top bar, switches twice as
  wide as tall, and black backgrounds.

### Apps and faces

- [ ] **Try the About app on the watch.** It has run only in the harness and the simulator.
  Put an icons file that carries its `info` icon on the watch first, then check the version,
  the uptime after the phone sets the clock, the reset reason after a button-hold reset, and the
  free storage before and after an install. On the About pages, check the Bluetooth address
  against the computer's, the flash, touch and accelerometer chips (a BMA425 here), that the
  touch panel's three ID registers read in one go, and that the task table's shares add up to
  something sensible. The bootloader page shows `CRC` until a bootloader with the version record
  is sent with the bootloader updater.
- [ ] **The display's state on the About app's pages,** as InfiniTime's System Info screen shows.
- [ ] **The full set of navigation turns.** Phones send about 50 icon names, such as forks,
  merges, ramps, ferries and roundabout exits, and the distance as text. BattleTime has 10 turns. Add
  InfiniTime's navigation pictures, map each name to one, and show the distance as the phone writes
  it.
- [ ] **Night icons in the weather app.** It shows the sun at night where InfiniTime shows the
  moon. The digital and PineTimeStyle faces use the shared `weather_icon_at`; the weather app and
  the lite-weather face still call `icon_named`.
- [ ] **Bebas Neue and Teko for Infineat,** which draws its time and date in them; BattleTime
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
- [ ] **Raw accelerometer recordings in a ring of their own,** so that a recording cannot push out
  months of the log ring.
- [ ] **Day blocks on the PineTime.** The step, heart rate and battery logs program their slots into
  blocks in place; check on the watch that a day costs no erase (the diagnostics' store counters),
  that a watch updated from day files keeps its history on the watch and in the companion, and that
  the record keys the old day files held come free as they age out.
- [ ] **A checkpoint of the store's tables,** so mounting need not read every ring record.
- [ ] **Use the file system:** save the BLE bond there.
- [ ] **Resource packages:** fonts and pictures installed from a zip over BLE, with a list of
  obsolete files that an upgrade deletes, as InfiniTime's `resources.json` has.
- [ ] **Report failed flash writes.** After each erase and page program, read the flash's security
  register and return an error when its erase-fail or program-fail bit is set, so the store refuses
  the write instead of trusting it.
### Firmware updates

- [ ] **Escaping the reloader under InfiniTime leads back to the reloader.** InfiniTime's DFU
  installs the reloader as an unconfirmed MCUBoot test image, and the reloader stages BattleTime over
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
  10 seconds; BattleTime only aborts when the link drops.
- [ ] **A swap progress bar on the bootloader screen.** It says "Installing firmware" for up to 30
  seconds with nothing moving. Its 20 KB has about 1.9 KB to spare.

### Developer tools and docs

- [ ] **A flaky app-host test.** `an_app_list_in_columns_lays_its_apps_out_as_square_cells` failed
  once in `app-host/src/fonts.rs` and passed on every rerun. The installed font is global, and
  `GeometricFont::bytes` reads its start and length separately while parallel tests install it,
  so a read can pair one install's start with an empty library's length. Read both in one step, or
  give each test its own font.
- [ ] **Measure the stack with any app open.** `tools/measure_stack.py --open-first-app` opens
  the launcher's first row, so with only the heart app installed it measures that: 7,080 B, 1,608 B
  spare, under the 2 KB the rest keep. The alarm's 10.5 KB peak and the About app's still cannot
  be measured again. Let it open a named app, and shrink the heart app's peak.
- [ ] **Clippy fails with warnings denied.** `cargo clippy --release -- -D warnings` stops on three
  `needless_borrow`s in `battletime-runtime`.
- [ ] **No MPU in pinetime-engine.** The engine models no memory protection unit, so the
  stack-guard test skips there.
- [ ] **A DFU upload tool in `tools/`** that sends a zip over BlueZ, as wasp-os's `dfu.py` does,
  including type `0x08` for the recovery slot, as `battletime recovery` does.
- [ ] **Flashing a watch over SWD.** One image with the bootloader and the firmware, a way to fill
  the recovery slot on a watch flashed this way, and a page on probes, the SWD pins and RTT logs.
- [ ] **Releases.** A version scheme, and a page saying what each firmware file in a release is
  for; `.github/workflows/release.yml` already publishes `dist/` and the store for each `v*` tag.
- [ ] **How to measure memory.** Say in [memory-map.md](memory-map.md) how each figure is measured
  on the watch: flash by crate and symbol, the stack's high-water mark, and heap peaks.
- [ ] **Firmware logic where tests reach it.** Coverage measures only the workspace. Move logic out
  of `firmware/` into workspace crates, as `boot` and the services already are, so host tests
  cover it.
- [ ] **Harness gaps.** Read the key labels of a
  `ButtonMatrix`, so `tap "7"` works on the calculator; match text spread over neighbouring
  labels, such as a face's "09:00"; set text system values from scripts; and see every built row of a repeated list, since `widget_text` and
  `widget_bounds` give only the first row's node, so `tap "Sam"` cannot reach the second row of
  the notifications list.
- [ ] **Scripts from an installed simulator.** `--script` finds app sources through the build-time
  `CARGO_MANIFEST_DIR`, so it works only through `cargo run` in the repository, not from
  `nix run`.
- [ ] **App host API for tests.** `widget_bounds` gives no clip rectangle, there is no iterator
  over the open screen's nodes, and there is no call to redraw the whole screen. The harness works
  around all three.
- [ ] **Step-by-step moves onto BattleTime** in the [guide](guide/updates-and-recovery.md): from
  InfiniTime and from wasp-os, and updating with Gadgetbridge or ITD.

### Not started

InfiniTime has these and BattleTime has nothing for them yet:

| Area | What InfiniTime has |
|---|---|
| Board and system loop | sleep and wake of the whole system, wake locks, a system monitor |
| Other BLE services | Music and Navigation in the shipped firmware |
| System screens | the pass key |

## Gaps against the wasp-os fork

What the BeatLink wasp-os fork has for each feature BattleTime already has, compared with its
`development` branch at `4f065f7` on 2026-10-11. Items already listed above are not repeated. The
full report is
[BattleTime vs wasp-os: feature coverage](https://claude.ai/code/artifact/1676468a-1a4f-421b-ac54-345934b08570).

### Display

- [ ] **Mute while switching apps,** so a half-drawn screen is never shown.
- [ ] **Measure the bus speed on the watch.** The fork found its SPI bus stuck at 1 Mbps instead of
  8 Mbps, which made every redraw 4.4 times slower.

### Accelerometer

- [ ] **Write the axis remap to the chip,** so its own motion features see the watch the right way
  up. The driver only remaps in software now.

### Bluetooth

- [ ] **The rest of the Gadgetbridge Bangle.js messages** in
  [companion-protocol.md](companion-protocol.md): alarms, steps and HTTP. `settings`, `notify`,
  `notify-`, calls, music, weather, `nav`, find, vibrate, status, `slots` and `state` are served.
- [ ] **A developer console over BLE,** to replace the fork's Python REPL for uploads, time setting
  and resets.

### UI toolkit

- [ ] **Widgets:** a digital clock and a stopwatch.
- [ ] **The rest of InfiniTime's grid launcher:** remember the last page, and grey out apps and
  faces whose resources are missing. Lists and grids, chosen in the companion, are done.

### Simulator

- [ ] **Run every app and face** in the simulator.
- [ ] **A smoke test** that opens every app in the simulator and checks its screenshot.

### External flash and storage

- [ ] **Updatable read-only image:** the fork keeps nearly all its code in a ROMFS image in
  internal flash, updated without reflashing the firmware.

### Not started

The fork has these and BattleTime has nothing for them yet; the faces, apps and companion features
BattleTime already ported are left out. Most also appear in the InfiniTime table.

| Area | What the fork has |
|---|---|
| Heart rate | raw data logging |
| Apps | Gallery, Self test |
| Companion app | WaspOSCompanion: themes, face previews |
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
  Decide whether BattleTime has a gesture for this, since several faces use the long press for their
  own settings.

### Developer tools

- [ ] **Screenshots over BLE.** PinetimeLite draws the current screen into the external flash and
  lets the phone read it back as RGB565. It would let the docs and bug reports show the real watch.

### Firmware updates

- [ ] **Update from a web page** with Web Bluetooth, as PinetimeLite's web app does: set the clock
  and send firmware and resources without installing anything.

## Other firmware to study

Not compared yet. Look through each for features worth bringing to BattleTime.

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
