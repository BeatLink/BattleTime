# Services

The services in `neotime/services/` keep the watch's state and speak its protocols: files in the
external flash, settings, the clock, the step and health logs, installed apps, updates, and what
the phone sends. Each is a `no_std` crate with no Bluetooth and no hardware in it, so its tests run
on a computer, most of them against the flash store in RAM. The [firmware](firmware.md) carries the
bytes between them and the radio, the screen and the flash.

## Storage

### Flash store (`services/flash_store`)

`neotime-flash-store` keeps the watch's files, records and logs in the 3 MB `FILE_SYSTEM` region of
the external flash, `async` and with no heap. It knows how data is written, not what it is, so a new
kind of data needs no new format:

- **Files** are named by path and written whole, at once or a part at a time as an upload arrives.
  Each is a list of up to 64 sectors in any order, committed by a header written last. Files of
  1 KB or less are kept as records. There are no folders; a path's slashes only name it.
- **Records** are small values rewritten often, keyed by their owner's app number, appended to a
  ring with a table in RAM of each key's newest copy. Writes are refused before the ring is half
  full, and each app may use 4 KB.
- **Logs** are appended over time, batched in RAM, and drop their oldest sector when the ring wraps.
- **`FileAccess`** is the trait the services reach files through, so they need not name the
  store's sizes; `ram_flash` (a feature) gives their tests a flash in RAM. Its
  `for_each_file_with_length` walks every file with its length, which folder listings are built
  from. The store's `file_extents` says where a part of a file lies, for a DMA read.

It survives a power cut at any write or erase, which its tests check exhaustively. Built for the
watch it takes 2,700 B of RAM and about 17 KB of code. [flash-store.md](flash-store.md) describes
the format, [storage.md](storage.md) what the watch keeps in it, and
[firmware.md](firmware.md#storage) how the firmware reaches it.

### Slot logs (`services/slot_log`)

`neotime-slot-log` keeps days of six-minute slots in the flash store, in blocks of whole days: blank
files of one sector, `/day_blocks/<log>/<n>`, block `n` starting `n` blocks of days after
1970-01-01. A slot is programmed into its place when it ends, with `program_file`, so it needs no
erase and a block costs one erase in all. Each byte is stored flipped by the log's mask, so a slot
never written reads as the log's empty value, and a slot keeps the first value written to it. A
block is deleted when a new day starts once all its days are older than `DAYS_KEPT` (90). Older
firmware kept each day as a file of its own, `<folder>/<year>/<month>-<day>`; those are read beside
the blocks and deleted once older than 90 days. The step log keeps its days here too, and two more
logs live here whole:

- **Heart rate,** `/logs/heart`: one byte a slot, the rounded average of the beats per minute the
  sensor measured in it (`Average`). A slot without a reading holds 0.
- **Battery,** `/logs/battery`: one byte a slot, the charge in percent at the slot's end, with the
  top bit set while charging. A slot the watch was off for holds `0xff`.

A block of 3,872 bytes holds 16 days of these, or 8 of steps. `SlotClock` says when a slot has
ended; the firmware's battery task holds it, as [firmware.md](firmware.md#battery) describes.

## Settings and time

### Settings (`services/settings`)

`neotime-settings` defines the watch-wide settings: the 12 or 24 hour clock, units, the step goal,
the brightness, the screen timeout, the always-on screen, chimes, do not disturb and sleep mode,
whether alarms ring in do not disturb, which touch wakes the screen, raise to wake, lower to sleep,
shake to wake and its threshold, Bluetooth, whether updates are taken, how often the heart rate is
measured in the background, the watch face, and whether the watch advertises under InfiniTime's
name for Gadgetbridge.

- **One place for limits.** Each `Setting` has a fixed number, the name apps pass to `set_setting`,
  and its lowest, highest and default values. `Settings::set` refuses a value outside them, so
  nothing that reaches the store, from an app or from the phone, can be out of range.
- **The file.** `/settings/system` holds a format byte, then one 5-byte record per setting: its
  number and its value as a little-endian `i32`. A record for an unknown setting or with a bad
  value is skipped, so a file from older or newer firmware still reads.
- **What a restart resets.** Bluetooth is written but never read back
  (`Setting::survives_restart`), so a restart always turns the radio on: every app, the Settings
  app included, is installed over Bluetooth, so a watch saved with it off could never be reached
  again. Updates allowed only until the next restart read back as off.
- **Apps.** `neotime-app-host`'s `settings` module maps each setting to the system value apps read,
  such as `settings.chimes`, and hands the open app a `settings_change` event for each change.

The theme is kept apart, as eight colours in `/settings/theme`, since the companion app can set any
colours; the Settings app and `settings.theme` pick and read it by preset number.

### Current Time (`services/current_time`)

`neotime-current-time` holds the wall clock's logic and turns its milliseconds since 1970 into the
Bluetooth Current Time service's two values and back, so the time handling is tested on a computer.

- **`WallClock`** keeps UTC as an offset from the timer's uptime, which the caller passes in, plus a
  `LocalTimeInformation` zone; it gives UTC and local time, and sets either, keeping UTC or the
  local time when the zone changes, as
  [companion-protocol.md](companion-protocol.md#time-and-time-zone) describes.
  `WallClock::set_by_hand` sets the local time the Settings app chose, given as a year, a day of it
  from 1 and a minute of that day, through `local_time_from_day_and_minute`, which refuses a day or
  minute that does not exist.
- **`parse_time_line`** reads Gadgetbridge's `setTime(...)` and `E.setTimeZone(...)` lines.
- **`encode_current_time`** gives the 10 bytes of 0x2A2B: the date, the time, the day of the week
  from Monday as 1, the 256ths of a second and an adjust reason of 0. `decode_current_time` reads a
  write of at least 7 bytes and refuses a year outside 1970 to 2200 or a date or time that does not
  exist.
- **`LocalTimeInformation`** holds 0x2A0F's time zone and daylight saving offset in quarter hours and
  refuses values the field does not allow, while taking its "unknown" values, which count as 0 in
  `offset_in_milliseconds`.

## The phone

### Companion messages (`services/companion`)

`neotime-companion` reads the companion app's messages and writes the watch's replies, as
[companion-protocol.md](companion-protocol.md) sets out: newline-ended JSON lines, `GB({...})` as
Gadgetbridge sends them or bare. `LineAssembler` joins writes into lines and drops one longer than
640 bytes; `parse_line` reads a message with `serde-json-core`, without allocating, and rounds
fractions as they arrive so a fractional temperature still reads.

| Message `t` | Read as |
|---|---|
| `settings` | A preset theme, single colours and any watch-wide setting, each checked against its limits; with none, a request for the current settings, answered by `settings_report`. |
| `notify` | A notification's id, app, title and body, unescaped and cut at a whole character to 24, 32 and 120 bytes. |
| `notify-` | The notification with this id was dismissed on the phone. |
| `call`, `musicinfo`, `musicstate` | A call starting, answered or ended; the player's track; whether it plays and where. |
| `weather` | Kelvin turned into Celsius, OpenWeatherMap codes into the sky names apps use. |
| `nav` | The next step of the phone's navigation, or its end. |
| `find`, `vibrate` | Ring the watch until stopped; buzz a number of times. Their one value, `n`, is read by hand, since a JSON parser for one field cost about 850 B each. |
| `status` | The phone asks for the battery and steps. |
| `slots` | Where the update and recovery slots are, or a request to move them (`slots.rs`). |
| `state` | The phone rewrote an app's saved state, so the app takes it up. |

Any other message is named by its `t`. `phone.rs` gives each of Gadgetbridge's messages its own
small struct, so its parse never adds to the others' stack. `PhoneUpdates` keeps the latest of each
kind until the shell takes them, the larger ones on the heap, and `PhoneData` keeps the music,
weather and navigation and answers the system values apps read them through. The crate also writes
the `status` reply and the `music`, `call`, `findPhone` and `request` lines an app's effects send.

`infinitime.rs` reads writes to InfiniTime's own GATT services in their byte formats into the same
messages: Simple Weather into `Weather` (with sunrise and sunset) and `Forecast`, and, with the
`single-values` feature, the music and navigation characteristics into one `PhoneField` each, which
`PhoneUpdates` keeps in order and `PhoneData` applies to the music or navigation it holds. It also
gives the music event for an app's control and the motion service's bytes. The firmware turns the
feature on only with its music or navigation service, so a build without them carries none of it.

It holds the standard services' content too, so the firmware only serves the bytes:

- **`advertising`**: the advertisement InfiniTime sends (flags, the Heart Rate and legacy DFU
  services, 0 dBm), the scan response with the name, `GADGETBRIDGE_NAME` ("InfiniTime"), and the
  intervals: fast, 20 to 30 ms, for 30 seconds after a start, a disconnect or Bluetooth coming back
  on, then about once a second.
- **`alert`**: a New Alert, written to the watch the way InfiniTime reads it or notified by a phone
  in the standard's layout, turned into a notification, or a call for the incoming call category.
- **`device_information`**: the Device Information strings, the same set InfiniTime serves, and
  the serial number from the chip's factory id.
- **`heart_rate_measurement`**: the Heart Rate service's value, a zero flags byte and the beats per
  minute in one byte.

### The phone's services (`services/phone_client`)

`neotime-phone-client` is the watch's GATT client for the phone's own Current Time and Alert
Notification services, which InfiniTime's ServiceDiscovery, CurrentTimeClient and
AlertNotificationClient use. `PhoneClient` is a state machine with no I/O: `next` takes the answer
to the last request and gives any value it read and the next request, so the firmware only carries
bytes and every step is tested on a computer against a pretend phone.

It finds each service with Find By Type Value, reads 0x2A2B and 0x2A0F with Read By Type, reads the
Alert Notification service's declarations a page at a time for New Alert and the control point,
finds New Alert's CCCD with Find Information, then writes the CCCD and the control point for every
category. An answer cut short at the end of the firmware's buffer is asked for again from the last
whole entry, and any other answer than the one expected ends that step. A phone that serves neither
service is left alone. `firmware/src/phone_services.rs` drives it; see
[firmware.md](firmware.md#the-phones-own-services) and
[bluetooth.md](bluetooth.md#the-phones-own-services).

### Notifications (`services/notifications`)

`neotime-notifications` keeps the notifications the watch holds, newest first: the latest 5 from
the phone and from apps, as InfiniTime keeps. Each one's text waits on the heap, about 200 B each,
so an empty list costs almost nothing, and a notification with the id of one already kept replaces
it, since the phone sends an update that way. Apps' own notifications take ids counting down from
-1, so none takes a phone's.

`NotificationList` answers the `notifications` and `unread_notifications` system values, and
carries out apps' `notify`, `dismiss_notification` and `mark_notifications_read`. It also gives
each notification an icon and a colour from the app that sent it: a call in the theme's success
colour, a missed call in danger, an email in warning, texts, chats and voicemail in a fixed blue,
calendar events in a fixed purple and the rest in the accent.

## Health and power

### Steps (`services/steps`)

`neotime-steps` keeps the step history the firmware serves to apps as `steps_today` and
`step_history`, and its log in `/steps`.

- **Readings.** `StepHistory::record` takes the chip's count and the local time. It adds what the
  count gained since the last reading, or the whole count when it is lower, which means the chip
  started again, to the six-minute slot being recorded, to that slot's hour and to the trip count.
  Steps found at a reading belong to the slot of the reading before.
- **Days.** It remembers today and the six days before as 24 hourly counts each, 336 bytes. At
  local midnight each day moves one back. A clock set to another day, such as by the phone after
  a restart, forgets them, and `log::store` reads them again from the log.
- **The log.** When a slot ends with steps, `log::store` programs them into that slot's place in the
  day's block, `/day_blocks/steps/<n>`, through the slot logs; a day older firmware kept as a file,
  `/steps/<year>/<month>-<day>`, is read beside its block. The trip count stays
  in RAM and is saved in `/steps/trip` when a day ends and when the screen goes dark, and only when
  it changed, through `log::save_trip_if_changed`. Each slot is a little-endian `u16`, as wasp-os's step logger writes them,
  so a full day is 480 bytes and a block holds 8 days. Slots without steps are never
  written, so a quiet night costs no flash writes. When a new day starts, `log::forget_old_days`
  deletes the blocks and files whose days are all older than 90, taken from the slot logs.
- **Start.** `log::load_history` reads the remembered days and the trip count back. The chip starts
  its count from 0 at every start, so a restart loses only the slot in progress, and the trip
  steps taken since the screen last went dark.

Midnight is local midnight, from the clock's time zone.

### Battery (`services/battery`)

`neotime-battery` turns each battery reading into the charge the watch shows. `Battery::measure`
moves a smoothed voltage a quarter of the way to each reading, reads the charge along InfiniTime's
measured curve, lets the charge only rise on the charger and only fall off it, caps it at 99 % while
charging, and calls the battery full, at 100 % and not charging, once two readings in a row find
power with the charge pin idle, until the charger is taken off.

Each reading also gives its events: the charger plugged in or taken off, the battery full, and the
charge below 10 % off the charger, once until the charger is next connected. `wakes_screen` says
which of them light the screen, given do not disturb.

For the bootloader, its updater and the reloader it turns a reading from
[`neotime-battery-reading`](drivers.md#battery-reading-driversbattery_reading) into the status
page's battery line, and allows a bootloader write only with power present and at least 3.7 V.

## Apps and updates

### Packages (`services/packages`)

`neotime-packages` is the package manager for the apps and watch faces on the external flash. Each
app has a folder, `/apps/<id>/`, with `app.ntapp`, its compiled image, and `state`, what it saved.
The folders are the whole record: there is no list of installed apps to fall out of step with them.

- **`Packages`** works on the mounted file system.
    - `install` checks an image already written to the app's folder with the same `AppImage::parse`
      and `verify` the app host uses, and refuses the shared library, an image whose id is not the
      folder's name, and one whose stored facts differ from a fresh analysis; a bad image is left
      where it is, turned off, so the phone can send it again. `remove` deletes the folder with the
      saved state.
    - `set_enabled` hides an app from the launcher with a `disabled` marker, without deleting it;
      `install` and `change_enabled` turn an app on or off only once `checks` allows it.
    - `list` returns every app whose image header holds its folder's id, in id order, with the heap
      it needs from the image's stored facts, reading only the header, the section table, the facts
      and three strings through `image_reader`; `enabled` gives the ones the launcher shows.
    - `read_icon` and `read_name` read an app's icon or name alone, for the launcher, and
      `runs_while_closed` reads only its subscriptions. `read_image` hands the shell an image to
      load into RAM; `read_image_in` reads one from `/temp/<id>/` as well, for a temporary app,
      which no listing sees. An image larger than the heap an app may have is never read: `list`
      gives it by its id, turned off, and `read_image` refuses it.
- **`faces`** makes a face's `watch_face` number from its id and picks the face to show, or the one
  a step on or back reaches.
- **`checks`** decides whether an app may be turned on or off, from the list alone. Turning one on
  is refused when its need, with the shell's index of app ids (and its full list of apps for an app
  that reads it), is over `APP_HEAP_BUDGET_BYTES`, or when it would push a launcher (an app with an
  `AppList` or reading `installed_apps`) over, since a launcher holds each listed app's name and
  icon and reads their images one at a time. Turning one off is refused for the launcher and the
  last face on. Other apps are not checked again when one is turned on, since only a launcher's need
  grows with it.
- **`commands`** answers the packages characteristic's JSON lines (`apps`, `install`, `remove`,
  `enable`, `launcher`, and `run` and `stop` for a temporary app) and says what changed, so the
  shell can read the apps again.
- **`launcher`** reads `/settings/launcher`, the bytes in which the companion says how the
  launcher looks and where every app goes, and `arrange` puts the listed apps in group order,
  any the file leaves out in a last group, Other. The shell keeps the file, the order and each
  group's run of it, which apps read as `launcher_groups`, and reads the names in place. It walks
  the bytes and the apps rather than parsing text and sorting, which keeps it near 1 KB of flash.
- **`library`** finds the shared library inside a firmware image, where its image header says
  (`firmware_library`), and reads the version it records. The library ships with the firmware and
  is never written on the watch.

### Updates (`services/updates`)

`neotime-updates` is the on-watch half of a firmware update, between the Bluetooth layer that
uploads an image and the bootloader that installs it. It knows nothing of the upload protocol:

- **`UpdateReceiver`** takes a firmware image with `begin(size)`, then `receive(chunk)` in order,
  then `finish()`. It writes through embassy-boot's `FirmwareUpdater` into the update slot, erasing
  each sector just before writing it. `finish` checks that every byte arrived, that the image's
  header is sound and linked for `FIRMWARE`, and that its CRC-32 matches, then marks a swap for the
  next reset. `read_back` lets the protocol above check its own checksum first.
- **`confirm_running_firmware`** tells the bootloader the running firmware works. Until it is called
  the next reset rolls back, and no new update is accepted.
- **`RecoveryReceiver`** writes the recovery firmware into its own slot the same way. It erases the
  start of the slot first and writes the header's magic last, so the bootloader never sees a
  half-written recovery firmware.
- **`change_slots`** moves the update and recovery slots by writing a slot table record, refused
  while a firmware is on trial. The companion's `slots` message reaches it.

### Firmware updates over Bluetooth (`services/dfu`)

`neotime-dfu` speaks Nordic's legacy DFU, the protocol InfiniTime uses, so Gadgetbridge, ITD, Siglo
and NeoTime's tools update NeoTime without changes. `LegacyDfu` is the state machine, with no
Bluetooth in it: control point commands and packet writes go in, notifications and a reset request
come out, and the bytes go to `neotime-updates`. `activity` reports the step it is on and
`Outcome::failure` why a step was refused, for the status page. Its 256-byte write buffer and
64-byte init packet are on the heap only while an update runs; a heap without room for them
answers `START_DFU` with `OPERATION_FAILED`.

An upload cut off by a lost link is kept. The same image sent again (the same type, size and init
packet) carries on from where it stopped for a phone that asks with `REPORT_RECEIVED_IMAGE_SIZE`
before the image; a phone that does not ask starts from the first byte. The `trouble` feature adds
the GATT service for TrouBLE, and the tests run whole updates through it. See
[updates.md](updates.md).

### Files over Bluetooth (`services/blefs`)

`neotime-blefs` speaks Adafruit's BLE file transfer protocol, InfiniTime's `FSService`, so ITD,
Gadgetbridge and the companion app reach the watch's files. `FileTransfer` turns one command into
its answers against the flash store's `FileAccess`; it remembers the file being read or written
between commands.

- **Folders.** The store has no folders, so a folder is the files whose paths start with it: making
  one always succeeds, and listing, deleting or moving one acts on what it holds.
- **Uploads.** Uploads of 1 KB or less are gathered in RAM and written whole, so they become
  records; larger ones stream into the store and replace the old file only when the last piece
  arrives, and one cut short resumes only while the store still holds it open. A move copies, then
  deletes.
- **Locked folders.** While updates are off the firmware locks `/apps` and `/temp`, so any write,
  delete, new folder or move that touches them, or the root above them, is refused, while reads
  and other folders work as before.

The `trouble` feature adds the GATT service. See
[companion-protocol.md](companion-protocol.md#file-transfer).

## Keeping the watch healthy

### Crash record (`services/crash_record`)

`neotime-crash-record` says why the watch last restarted: the chip's `RESETREAS` bits in words
(`RestartReason`), and a record of the last panic, out-of-memory or HardFault. The record is 60
bytes of little-endian words in RAM that a reset keeps: `CRSH`, the kind, eight detail words, 16
bytes of the open app's id, and a CRC-32 of the rest, so RAM that came up with noise never reads as
a crash. `last_restart_words` gives the words the diagnostics characteristic carries. `screen` lays
out and draws the crash page and the page shown when an app needs more memory than is free, in the
status page's font, so they need nothing from the shared library. How the firmware fills the record
is in [firmware.md](firmware.md#crashes).

### Liveness (`services/liveness`)

`neotime-liveness` judges whether the shell and the radio still make progress, so the watchdog is
fed only while they do. The watchdog task checks twice a second with each part's counter: the
shell's loop count, and the radio's interrupt count while it advertises or holds a connection. A
quiet shell is asked to prove it still runs (`ping_shell`). A shell stopped for six checks, or a
radio with work stopped for twelve, is judged stopped. Checks are counted rather than timed, so a
long blocking job that also stops the watchdog task never counts against a part.

### Foreground (`services/foreground`)

`neotime-foreground` says whether the wearer is watching the screen move, so background work that
blocks every task waits for a pause. `may_start` holds work back while the shell is part way
through a transition or a sliced redraw, or a finger is down, but never longer than a second, so a
screen kept busy cannot starve it. The firmware's use of it is in
[firmware.md](firmware.md#background-work-waits-for-a-pause).

### Task table (`services/task_table`)

`neotime-task-table` keeps the firmware's tasks, with how often each has run and how long it has
spent running, for the System app's Tasks page. The firmware feeds it from embassy's trace hooks: a
task is added when it is spawned, timed each time the executor polls it, and removed when it ends. A
task names itself on its first run, since the hooks know a task only by its address.
