# Companion protocol

How a phone or computer talks to a NeoTime watch over Bluetooth Low Energy. The NeoTime Companion
app mirrors this page in its `src/protocol`; this page is the single source for both. Each part says
whether the watch serves it yet.

## Finding the watch

- The watch advertises the complete local name `NeoTime`. The advertisement has no room for a
  128-bit service UUID.
- After connecting, a NeoTime watch is recognised by the NeoTime service
  `6e540001-0000-4000-8000-4e656f54696d`, which every build serves, recovery included.
- A client should check which of the services below are present and offer only the features they
  support, since older firmware serves fewer.

## Standard services

| Service | UUID | Characteristic | Use | Served |
|---|---|---|---|---|
| Legacy DFU | `00001530-1212-efde-1523-785feabcd123` | as Nordic's legacy DFU | Firmware updates; see [updates.md](updates.md). | Yes |
| Current Time | `0x1805` | `0x2A2B`, read and write | Writing the 10-byte Current Time value sets the clock to that local time, read in the zone the watch already has; reading it gives the watch's local time, with the day of the week and 256ths of a second. A write that is not a real date and time is ignored. See [Time and time zone](#time-and-time-zone). | Yes |
| | | `0x2A0F`, read and write | Local Time Information: the time zone and daylight saving offset, in quarter hours. Writing it changes the zone and keeps the local time; reading it gives the zone the watch is in, or 0 and 0 until one is written. A value outside the field's range is ignored. Saved, so it survives a restart. | Yes |
| Battery | `0x180F` | `0x2A19`, read and notify | The charge, 0 to 100, notified when it changes. | Yes |
| Alert Notification | `0x1811` | `0x2A46` New Alert, write | Shows a notification, read as InfiniTime reads it: a category byte, two more header bytes, then a title, a zero byte and the body, or only a body when there is no zero byte; at most 125 bytes. The category names the notification's app (`Email`, `Call`, `SMS`, `Messages` and the rest of the standard's list; `Alert` for any other). A write with no text is ignored. Each one is kept as a new notification, as in [Notifications](#notifications-served). | Yes |
| File transfer | `0xFEBB` | as in [File transfer](#file-transfer) | Reading, writing, listing, moving and deleting files. | Yes |

## Time and time zone

The watch counts UTC and keeps a time zone and daylight saving offset beside it. Local time is UTC
plus both offsets, and an unknown time zone (`-128`) or daylight saving (`255`) counts as 0. Apps
see UTC as `now` and the sum as `utc_offset`; faces, alarms, chimes and the midnight step reset use
local time. The zone is saved in `/settings/time_zone` as the two bytes of 0x2A0F; the time itself
starts again from the build time after a restart, so a phone should set it on every connection.

The two ways in treat the zone differently, as their senders expect:

- **Current Time Service.** 0x2A2B carries local time, as the standard says, so the watch keeps
  UTC as that time minus its zone. A 0x2A0F write keeps the local time and moves UTC instead. A
  phone may write the two in either order, as InfiniTime allows, and the watch ends at the same
  time. A client that writes only 0x2A2B leaves the zone as it was, so its local time is right but
  UTC is off by that zone.
- **Gadgetbridge's Bangle.js line.** A message channel line starting `setTime(` or
  `E.setTimeZone(`, optionally after the `0x10` byte Gadgetbridge puts first, is read for those
  two calls, each at the start or after a `;`, and the rest of the line is ignored.
  `setTime(<seconds since 1970, UTC>)` sets UTC, and `E.setTimeZone(<hours>)` sets the whole offset,
  rounded to a quarter hour, as the time zone with no daylight saving, keeping UTC. A call whose
  value is malformed or out of range is skipped. The watch sends no reply. For example
  `\x10setTime(1767603600);E.setTimeZone(1.0);...` sets 10:00 local time at UTC+1.

## The message channel

Messages travel over the Nordic UART service, as Gadgetbridge's Bangle.js driver sends them:

| Characteristic | UUID | Direction |
|---|---|---|
| Service | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | |
| Receive | `6e400002-b5a3-f393-e0a9-e50e24dcca9e` | Phone writes, with or without response |
| Transmit | `6e400003-b5a3-f393-e0a9-e50e24dcca9e` | Watch notifies |

- Each message is one line of JSON ending in a newline. The phone sends `GB({...})\n`, as
  Gadgetbridge does; a bare `{...}\n` is read the same. The watch sends bare `{...}\n`.
- Every message is an object whose `t` names it.
- Lines are ASCII: write any other character as `\uXXXX`.
- A line may take several writes, and a reply several notifications of up to 20 bytes each; the
  reader joins them until the newline. Lines are at most 512 bytes; the watch drops a longer one.
- The watch ignores keys it does not know, and messages it does not handle yet. It answers a line
  it cannot read with `{"t":"error","msg":"..."}`.

### Settings (served)

`{"t":"settings", ...}` changes the keys it carries and leaves the rest. With no keys besides `t`
it changes nothing. Either way the watch answers with the settings as they now are.

| Key | Value |
|---|---|
| `theme` | A preset's name: `dark`, `light`, `ocean`, `forest`, `sunset` or `contrast`. `custom` keeps the current colours. |
| `theme_colors` | An object with any of the eight theme colours, each `"#rrggbb"`: `background`, `surface`, `text`, `muted`, `accent`, `success`, `warning`, `danger`. Applied after `theme`, so a preset can be adjusted in one message. |
| `brightness` | The backlight level, 1 to 3, used whenever the open app has not set its own. |
| `blank_after` | Seconds without input before the screen turns off, 5 to 300. |
| `clock_24h` | `true` for a 24 hour clock, `false` for 12 hours. |
| `units` | `"Metric"` or `"Imperial"`. |
| `step_goal` | The daily step goal, 1000 to 50000. |
| `always_on` | `true` to dim the screen instead of turning it off. |
| `chimes` | `0` off, `1` a buzz every hour, `2` every half hour. |
| `do_not_disturb` | `true` to silence chimes, notifications and every vibration but alarms. |
| `bluetooth` | `false` drops the phone and stops advertising until the wearer turns it on again or the watch restarts. |
| `updates` | `false` refuses firmware updates until turned on again. |
| `wake_on_touch` | What turns a dark screen on besides the button: `0` nothing, `1` a tap, `2` a double tap. |
| `alarms_in_dnd` | `false` silences alarms and timers in do not disturb too. |
| `heart_rate_interval` | How often the watch measures the heart rate with no app asking: `0` off, `1` continuously, then `2` to `6` for every 30 seconds, minute, 5, 10 or 30 minutes. |

The key names after `theme_colors` are the ones the wasp-os companion's settings screen already
sends. An unknown preset, a colour not written `#rrggbb`, `units` other than the two names, or any
value outside its range is refused with an `error` reply, and nothing in that message is applied.

The reply:

```json
{"t":"settings","theme":"dark","theme_colors":{"background":"#000000","surface":"#292c31",
 "text":"#ffffff","muted":"#8c929c","accent":"#00aaa5","success":"#39be5a","warning":"#f7aa18",
 "danger":"#e74539"},"themes":["dark","light","ocean","forest","sunset","contrast"],
 "clock_24h":true,"units":"Metric","step_goal":10000,"brightness":2,"blank_after":15,
 "always_on":false,"chimes":0,"do_not_disturb":false,"bluetooth":true,"updates":true,
 "wake_on_touch":1,"alarms_in_dnd":true,"heart_rate_interval":0}
```

`theme` is the preset whose colours match exactly, or `custom`. Colours are stored as RGB565, so a
colour reads back to the nearest one the screen shows. The watch keeps the theme in
`/settings/theme` and every other setting in `/settings/system`, so they survive a restart. The same
settings change from the watch's own Settings app; the phone sees those changes in the next reply.

### Notifications (served)

`{"t":"notify","id":1234,"src":"Messages","title":"Sam","body":"On my way"}` is a notification
that arrived on the phone. The watch buzzes twice, unless do not disturb is on, and hands it to the
open app's `notification` subscriptions, with `src` as its `app`. Escapes such as `\n` and `\"` are
undone, and the watch keeps up to 24 bytes of `src`, 32 of `title` and 120 of `body`, cut at a whole
character. Gadgetbridge's other keys, such as `sender`, are ignored. The watch keeps the latest 5,
newest first, for apps to read as `notifications`; one with the `id` of one already kept replaces
it. While the screen is dark, outside do not disturb, it turns the screen on and opens the
notifications. It sends no reply.

`{"t":"notify-","id":1234}` says the notification with that `id` was dismissed on the phone; the
watch removes it from the list, and sends no reply.

### Calls, music, weather and the rest (served)

These follow Gadgetbridge's Bangle.js messages, so its driver works too. Numbers are whole; a
field of the wrong type, such as a temperature with a fraction, makes the line unreadable, which
the watch answers with an `error` reply. Text is unescaped and cut at a whole character.

| `t` | Fields the watch reads | What it does |
|---|---|---|
| `call` | `cmd`, `name`, `number` | Fills `current_call` and runs the `call` subscriptions with the caller's `name`, or the `number` without one, up to 32 bytes. `incoming` is an incoming call, which opens the `call` app and rings until the call is answered, ended or controlled from the watch, unless do not disturb is on; without that app it buzzes once and turns the screen on. `accept`, `start` and `outgoing` are `answered`; `reject` and `end` are `ended`, and both stop the ring. Another `cmd` is ignored. |
| `musicinfo` | `artist`, `track`, `dur` (seconds) | Fills `current_music`'s artist (32 bytes), track (64 bytes) and length, and runs the `music` subscriptions. `album` is ignored. |
| `musicstate` | `state`, `position` (seconds) | `play` is playing and anything else is not; `position`, when given, is how far into the track. Runs the `music` subscriptions. |
| `weather` | `temp`, `hi`, `lo` (kelvin), `code`, `loc` | Fills `current_weather`: the temperatures in Celsius, the sky from the OpenWeatherMap `code`, the place (32 bytes) and `received_at`, the watch's time when it came. Runs the `weather` subscriptions. The watch has no sunrise, sunset or coming days from it. `hum`, `txt`, `wind` and the rest are ignored. |
| `find` | `n` | `true` rings the watch, even in do not disturb, for up to a minute or until the wearer touches the watch or presses the button; `false` stops it. |
| `vibrate` | `n` | Buzzes `n` times, 1 to 5, unless do not disturb is on. |
| `status` | | Replies `{"t":"status","bat":87,"mv":3950,"steps":4321,"charging":false}`: the charge in percent, the battery's voltage in millivolts, smoothed as `battery_millivolts` is, today's steps and whether it is charging. |

The watch keeps the music and weather until it restarts, and the latest of each message until the
shell takes it, so a burst of messages leaves only the newest of each kind.

Watch to phone, sent as apps ask: `music` with `n` (`play`, `pause`, `next`, `previous`,
`volumeup`, `volumedown`) for `music_control`; `call` with `n` (`ACCEPT`, `REJECT`, `IGNORE`) for
`call_control`'s `answer`, `reject` and `mute`, which also stop the watch ringing (the phone
silences its ringer for `IGNORE` and leaves the call ringing); `findPhone` with `n`, `true` or `false`, for
`find_phone`; and `app` for an app's `send_to_phone(channel, data)`:
`{"t":"app","ch":"<channel>","d":{"<key>":<number>,...}}`. The channel and keys are 1 to 32 letters,
digits, `_` or `-`, written out in the app; `d` holds up to 4 whole numbers, in the order the app
wrote them, or is `{}`. For example the remote app's third button sends
`{"t":"app","ch":"remote","d":{"button":2}}`. The phone sends no reply.

### Slots (served)

`slots` reports or moves the update and recovery slots in the external flash, which the
[slot table](bootloader.md#the-slot-table) records. The full firmware serves it; the recovery
firmware answers `{"t":"slots","err":"the recovery firmware cannot move the slots"}`, since the
reloaders carry it in wasp-os's app slot and it has no room to spare.

| Line | Answer |
|---|---|
| `{"t":"slots"}` | `{"t":"slots","update":[0,401408],"recovery":[413696,397312],"area":1048576,"firmware":397312,"image":171428,"restart":false}` |
| `{"t":"slots","recovery":[start,size]}`, with `update` as well or instead | The same report naming the new slots, with `"restart":true`; the watch then restarts |

Each slot is `[start, size]` in bytes; one left out stays where it is. `area` is where the file
system starts, which every slot must end by; `firmware` is the firmware slot's size, and the update
slot must be exactly one 4096-byte page more; `image` is the recovery image's length with its
header, or 0 when the slot holds none. The watch checks the slots as the bootloader would (whole
pages, before `area`, a recovery slot between 64 KB and `firmware`, no overlap). It then moves the
recovery image to a recovery slot that starts somewhere new, reading the copy back, so the table
never names an empty slot when there was an image to keep; a slot that keeps its start but shrinks
keeps the image where it is. Only then does it write the table, and it restarts so its update and
recovery receivers use the new slots. Moving the update slot drops the firmware a rollback would
use. A refusal answers `{"t":"slots","err":"..."}` and changes nothing, for example while an update
waits or is on trial, when the image is larger than the new slot, or when the new recovery slot
overlaps the old one at another start.

The companion's command line shows them with `neotime slots`, shrinks the recovery slot to its
image with `neotime slots fit`, and moves one with `neotime slots recovery <start> <size>` or
`neotime slots update <start>`.

### Other messages (not served yet)

Phone to watch:

| `t` | Fields | Meaning |
|---|---|---|
| `alarm` | `d`: a list of `{h, m, on, rep}` | Set the alarms; with no `d`, report them. |
| `steps` | `y`, `m`, `d` | The watch replies with that day's steps in 240 six-minute slots, `null` where unknown. |
| `http` | `id`, `resp` or `err` | The answer to a watch's `http` request. |

Watch to phone: `http` with `id`, `url`, `method`, `body`; `info` with `msg`.

## File transfer

Files move through Adafruit's BLE file transfer protocol, the one InfiniTime's `FSService` serves,
so ITD, Gadgetbridge and InfiniTime's resource loader work with NeoTime unchanged.
`services/blefs` speaks it.

| Characteristic | UUID | Use |
|---|---|---|
| Service | `0xFEBB` | |
| Version | `adaf0100-4669-6c65-5472-616e73666572` | Read: the protocol version, 4, as a little-endian `u32`. |
| Transfer | `adaf0200-4669-6c65-5472-616e73666572` | Write or write without response: one command. Notify: its answers. |

Numbers are little-endian, paths are absolute (`/apps/stopwatch/app.ntapp`), and every command fits
in one write, so the phone must exchange a larger MTU first. The watch's packets allow an MTU of
up to 124. An answer's second byte is its status: `1` OK, `2` error, `3` no such file, `4` protocol
error (a command cut short, or data with no write in progress).

| Command | Layout | Answer |
|---|---|---|
| Read `0x10` | `0x10`, pad, path length `u16`, offset `u32`, chunk size `u32`, path | `0x11`, status, pad ×2, offset `u32`, total length `u32`, chunk length `u32`, data |
| Read pacing `0x12` | `0x12`, `1`, pad ×2, offset `u32`, chunk size `u32` | the next `0x11` |
| Write `0x20` | `0x20`, pad, path length `u16`, offset `u32`, time `u64`, total size `u32`, path | `0x21`, status, pad ×2, offset `u32`, time `u64`, bytes it will take next `u32` |
| Write data `0x22` | `0x22`, `1`, pad ×2, offset `u32`, length `u32`, data | `0x21` with the offset of the next write |
| Delete `0x30` | `0x30`, pad, path length `u16`, path | `0x31`, status |
| Make folder `0x40` | `0x40`, pad, path length `u16`, pad ×4, time `u64`, path | `0x41`, status, pad ×6, time `u64` |
| List folder `0x50` | `0x50`, pad, path length `u16`, path | one `0x51` per entry: status, name length `u16`, entry `u32`, total `u32`, flags `u32` (bit 0: folder), time `u64`, size `u32`, name; then a last `0x51` whose entry equals the total and whose name is empty |
| Move `0x60` | `0x60`, pad, old length `u16`, new length `u16`, old path, one separator byte, new path | `0x61`, status |

- **Reading.** Each answer holds as much of the asked-for chunk as fits in one notification. The
  phone asks for the rest with read pacing, from the offset it has reached, until it has the total.
- **Writing.** A write at offset 0 starts the file anew; the phone then sends data pieces in order,
  and the last answer says 0 bytes to come. The old file stays until the last piece arrives, so an
  upload cut short never leaves half a file. A write at a later offset resumes an upload only while
  the watch still holds that upload open, at that offset; otherwise it is refused with status `2`,
  and the phone starts again from 0. A piece sent again is taken as it was the first time.
- **Folders** exist only through the files in them, so making a folder always succeeds, and a
  folder listed after it is made but before a file is put in it reports no file. **Deleting** or
  **moving** a folder acts on every file in it; a move copies, then deletes.
- The watch keeps no file times, so every time it reports is 0.
- **Differences from InfiniTime.** InfiniTime never cuts a file back when it is written again,
  answers a data piece with that piece's own offset, and reports some failures as negative littlefs
  codes. NeoTime follows Adafruit's description: status `3` for a missing file, the next offset in
  write answers, and a written file that ends at its total size.

Each app keeps its folder under `/apps/<id>/`: `app.ntapp` is the compiled image, as
`neotime-compiler` writes it, and `state` is its saved state. A temporary app's image and data go
in `/temp/<id>/`, as [Temporary apps](#temporary-apps-served) describes.

### Notes

The notes app shows `/notes/note.txt`, read only. The companion app writes it whole with the
file transfer above whenever the chosen file is new or has changed, and writes `/notes/font` with
the name of the font to show it in: `small`, `normal`, `medium` or `large`, without quotes. Any other
font file, or none, leaves the app's own font. The text is UTF-8; the watch shows tabs and carriage
returns as spaces and other bytes as `?`, and draws only the characters its fonts have. Deleting
`/notes/note.txt` brings back the app's hint to choose a file.

### Launcher

The launcher groups the apps as the companion app sets up, in `/settings/launcher`, which the
companion writes whole with the file transfer above and then follows with `{"c":"launcher"}` on the
packages characteristic, so the open launcher shows the change at once. The file says where every
app goes; the companion places each app in the group its image declares (the last field of the
`apps` answer) until the wearer moves it, and writes the file again after installing an app.

| Bytes | Holds |
|---|---|
| 0 | The format version, `1`. |
| 1 | The style: 0 one list, 1 folders, 2 sections, 3 pages. |
| 2 | The view: 0 rows, 1 a grid. |
| 3 | How many groups follow, in the launcher's order. |
| then, per group | Its icon as its place in the shared library's icons (255 for none, which shows its first app's), its id's length and id (such as `games`, kept for the companion; the watch skips it), its name's length and the name in UTF-8, how many apps it holds, then each app's id length and id, in the order the group shows them. |

Apps the file does not name, such as one installed another way, go in a group of their own at the
end, named Other. Ids of apps that are not installed are skipped, and a group with no installed
app is not shown. A group cut short ends the groups there. With no file, or one of another
version, the launcher shows one plain list.

### Remote

The remote app shows the buttons the companion app chose, and the companion app runs each one's
action when the watch says it was tapped. The companion app writes `/remote/buttons` whole with
the file transfer above whenever the buttons change:

| Byte | Holds |
|---|---|
| 0 | The format version, `1`. |
| 1 | How many buttons, `N`, 0 to 9. |
| 2 to 2 + N - 1 | Each button's icon, as its place in the shared library's icons, `ICON_NAMES` in `compiler/src/types.rs` (`phone` is 31, `play` 32). |

The watch reads the file when the app opens and when the screen turns on. A missing file, another
version or no buttons show a hint to choose buttons in the companion app; a count above 9 shows 9,
and an icon beyond the library shows none. A tap sends `{"t":"app","ch":"remote","d":{"button":i}}`,
`i` being the button's place in the file counting from 0.

## The NeoTime service

Service `6e540001-0000-4000-8000-4e656f54696d`, with its characteristics on the same base:

| Characteristic | UUID | Use | Served |
|---|---|---|---|
| Diagnostics | `6e540002-…` | Read: eight little-endian `u32` counters, then fourteen words on the last restart: the `RESETREAS` bits, the kind of crash (0 none, 1 panic, 2 HardFault, 3 out of memory), eight detail words and four words holding the open app's id, zero-padded, then four more counters: microseconds spent drawing apps, microseconds spent sending to the display, sectors the flash store erased, and store calls the shell made while another task held the store; see [architecture.md](architecture.md). Write `01` once a debug report holding the crash is saved: the watch forgets the crash and leaves its crash screen for the face. | Yes |
| Packages | `6e540003-…` | Write and notify: one JSON command per line, as in [Packages](#packages-served). | Yes |

### Packages (served)

An app or face is sent as a file first, to `/apps/<id>/app.ntapp`, then installed with a command on
the packages characteristic. Each command is a JSON line written to it, and each answer a JSON line
notified on it, in pieces of up to 20 bytes; a failure answers with the command and `"err":"..."`. The
recovery firmware has no apps, so it refuses every command with an error.

| Command | Answer |
|---|---|
| `{"c":"apps"}` | `{"c":"apps","room":budget,"a":[[id,name,kind,enabled,bytes,need,group],...]}`, `kind` being `app` or `face`, `enabled` 0 or 1, `need` the heap in bytes the app needs while open and `group` the launcher group its image declares, empty when none, in id order; `room` is the most an app may need. Each image's CRC and id are checked, but its code only at install. An image larger than `room` is never read: it is listed by its id, as an app, turned off, with `need` its length. The watch sends the line a piece per app, so a long list never needs one large block of its heap |
| `{"c":"install","id":id}` | `{"c":"install","ok":1,"need":need,"room":budget}` once `/apps/<id>/app.ntapp` passes the app checks, holds that id, is not the shared library, was built against no newer shared library than the one inside the running firmware, and fits the heap, as `enable` checks; the app then appears in `installed_apps`. An app that needs a newer library or does not fit stays installed but turned off, and the answer is a refusal |
| `{"c":"remove","id":id}` | `{"c":"remove","ok":1}`; deletes `/apps/<id>` with its saved state |
| `{"c":"enable","id":id,"on":bool}` | `{"c":"enable","ok":1,"need":need,"room":budget}`; a disabled app stays installed but leaves the launcher. `on` is true when left out |
| `{"c":"launcher"}` | `{"c":"launcher","ok":1}`; reads `/settings/launcher` again and regroups the launcher, as [Launcher](#launcher) describes |
| `{"c":"run","id":id}` | `{"c":"run","ok":1,"need":need,"room":budget}` once the shell has opened the temporary app in `/temp/<id>`, as [Temporary apps](#temporary-apps-served) describes |
| `{"c":"stop"}` | `{"c":"stop","ok":1}`; closes the temporary app if one is open |

Before `install` and `enable` turn an app on, the package manager checks that it fits the heap
beside everything else, and before `enable` turns one off that the watch keeps a launcher and a
face. A refusal answers `{"c":...,"err":text,"why":number}`, with `"need":need,"room":budget` too
when memory is the reason. The numbers are the same as the `refusal` apps read in `packages`:

| `why` | `err` | Meaning |
|---|---|---|
| 1 | `it needs more memory than an app can have` | Its need, with the shell's index of app ids (and its full list for an app that reads it), is over `room` |
| 2 | `the launcher would run out of memory` | The launcher's need, which grows with each app it lists, would go over `room`; `need` is the launcher's |
| 3 | `the launcher stays on` | The launcher is how every other app is reached |
| 4 | `the last watch face stays on` | Another face must be on first |
| 5 | `it is open on the watch` | Only the watch's own Software app meets this; the packages characteristic never answers it |

[memory-map.md](memory-map.md#how-much-heap-an-app-needs) explains how the need is worked out.

### Temporary apps (served)

A temporary app is opened on the watch without being installed: a question to answer, a call
screen, a game's score. It never joins the launcher, the Software app or the `apps` answer, and
never saves state.

1. The phone writes the image to `/temp/<id>/app.ntapp` with the file transfer above, `<id>` being
   the id inside the image, and any data files the app reads beside it, such as
   `/temp/prompt/text`. The app reads them by path with `file_bytes` or `TextFile`, as any app reads
   a file.
2. `{"c":"run","id":id}` opens it over whatever shows and turns the screen on. The going back is
   recorded as for any screen, so the side button, or the app going back, returns to what it
   covered. Running it again while it shows starts it afresh, reading its data files again.
3. It closes, going back to what it covered, on `{"c":"stop"}` and when the phone disconnects.

The shell, not the storage task, answers `run`, once it has opened the app, so the answer comes
after the app is on screen. The shell reads and checks the image as it does any app's. When the app
will not open, the wearer stays on the screen they had, and the answer says why:

| Answer | Why |
|---|---|
| `{"c":"run","err":"the image is missing or would not open"}` | No image in the folder, or one that fails the app checks, holds another id, or needs a newer shared library |
| `{"c":"run","err":"it needs more memory than an app can have","why":1,"need":need,"room":budget}` | Its need, less the image already read, is more than the heap has free beside the shell |
| `{"c":"run","err":"an app id is letters, digits, - and _"}` | A malformed id |

`stop` always answers `ok`, whether or not a temporary app was open. The files stay in `/temp`
until the phone deletes them, so a phone can keep an image there and later write only its data
before each `run`; the companion compares the image's first 20 bytes, which hold its length and
CRC, before sending it again. To forget an app, the phone sends `stop`, then deletes its folder with
the file transfer's delete. A restart opens nothing from `/temp`. A watch face's image runs like an
app's; the companion sends only apps.

The companion's `neotime apps run <image> [--data <name>=<file>] [--text <name>=<text>]`,
`neotime apps stop` and `neotime apps forget [id]` do these steps, and `neotime ask "<question>"`
runs the `prompt` app with the question as `text` and prints the answer: the app sends
`{"t":"app","ch":"prompt","d":{"answer":1}}` for Yes and `0` for No, then goes back.

An id is 1 to 32 letters, digits, `-` and `_`. Each app keeps its folder under `/apps/<id>/`:
`app.ntapp` is the compiled image, as `neotime-compiler` writes it, and `state` is its saved state.
The shared library is a single image that `neotime-compiler library -o shared.ntlib` writes, and
`tools/build_images.sh` puts inside the firmware image.

## Where the code lives

- `services/companion`: reads lines and messages and writes replies, with no Bluetooth in it, so
  its tests run on a computer.
- `services/packages`: installs, lists and removes apps, and finds the shared library inside the
  firmware image, with no Bluetooth in it.
- `firmware/src/main.rs`: the GATT services, which hand lines to `services/companion` and send its
  replies.
- `services/settings`: each setting's limits and default, and the `/settings/system` file.
- `firmware/src/settings.rs` and `firmware/src/shell.rs`: apply settings and keep them on the
  external flash.
