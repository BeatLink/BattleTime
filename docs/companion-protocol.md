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
 "wake_on_touch":1,"alarms_in_dnd":true}
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
notifications app. It sends no reply.

`{"t":"notify-","id":1234}` says the notification with that `id` was dismissed on the phone; the
watch removes it from the list, and sends no reply.

### Calls, music, weather and the rest (served)

These follow Gadgetbridge's Bangle.js messages, so its driver works too. Numbers are whole; a
field of the wrong type, such as a temperature with a fraction, makes the line unreadable, which
the watch answers with an `error` reply. Text is unescaped and cut at a whole character.

| `t` | Fields the watch reads | What it does |
|---|---|---|
| `call` | `cmd`, `name`, `number` | Runs the `call` subscriptions with the caller's `name`, or the `number` without one, up to 32 bytes. `incoming` is an incoming call, which buzzes and turns the screen on unless do not disturb is on; `accept`, `start` and `outgoing` are `answered`; `reject` and `end` are `ended`. Another `cmd` is ignored. |
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
`call_control`'s `answer`, `reject` and `mute`; `findPhone` with `n`, `true` or `false`, for
`find_phone`; and `app` for an app's `send_to_phone(channel, data)`:
`{"t":"app","ch":"<channel>","d":{"<key>":<number>,...}}`. The channel and keys are 1 to 32 letters,
digits, `_` or `-`, written out in the app; `d` holds up to 4 whole numbers, in the order the app
wrote them, or is `{}`. For example the remote app's third button sends
`{"t":"app","ch":"remote","d":{"button":2}}`. The phone sends no reply.

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
- **Writing.** A write opens the file, makes any missing folders above it, and cuts the file back
  to the offset, so an interrupted upload resumes where it stopped and a new one starts at 0. The
  phone then sends data pieces in order. The last answer says 0 bytes to come.
- **Deleting** a folder deletes everything in it. **Making a folder** makes its parents and
  succeeds if the folder is already there.
- The watch keeps no file times, so every time it reports is 0.
- **Differences from InfiniTime.** InfiniTime never cuts a file back when it is written again,
  answers a data piece with that piece's own offset, and reports some failures as negative littlefs
  codes. NeoTime follows Adafruit's description: status `3` for a missing file, the next offset in
  write answers, and a written file that ends at its total size.

Each app keeps its folder under `/apps/<id>/`: `app.ntapp` is the compiled image, as
`neotime-compiler` writes it, and `state` is its saved state.

### Notes

The notes app shows `/notes/note.txt`, read only. The companion app writes it whole with the
file transfer above whenever the chosen file is new or has changed, and writes `/notes/font` with
the name of the font to show it in: `small`, `normal`, `medium` or `large`, without quotes. Any other
font file, or none, leaves the app's own font. The text is UTF-8; the watch shows tabs and carriage
returns as spaces and other bytes as `?`, and draws only the characters its fonts have. Deleting
`/notes/note.txt` brings back the app's hint to choose a file.

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
| Diagnostics | `6e540002-…` | Read: eight little-endian `u32` counters, then ten words on the last restart: the `RESETREAS` bits, the kind of crash (0 none, 1 panic, 2 HardFault) and eight detail words; see [architecture.md](architecture.md). | Yes |
| Packages | `6e540003-…` | Write and notify: one JSON command per line, as in [Packages](#packages-served). | Yes |

### Packages (served)

An app or face is sent as a file first, to `/apps/<id>/app.ntapp`, then installed with a command on
the packages characteristic. Each command is a JSON line written to it, and each answer a JSON line
notified on it, in pieces of up to 20 bytes; a failure answers with the command and `"err":"..."`. The
recovery firmware has no apps, so it refuses every command with an error.

| Command | Answer |
|---|---|
| `{"c":"apps"}` | `{"c":"apps","a":[[id,name,kind,enabled,bytes],...]}`, `kind` being `app` or `face` and `enabled` 0 or 1, in id order; each image's CRC and id are checked, but its code only at install |
| `{"c":"install","id":id}` | `{"c":"install","ok":1}` once `/apps/<id>/app.ntapp` passes the app checks, holds that id and is not the shared library; the app then appears in `installed_apps` |
| `{"c":"remove","id":id}` | `{"c":"remove","ok":1}`; deletes `/apps/<id>` with its saved state |
| `{"c":"enable","id":id,"on":bool}` | `{"c":"enable","ok":1}`; a disabled app stays installed but leaves the launcher |
| `{"c":"install_library"}` | `{"c":"install_library","ok":1}` once `/resources/shared.ntlib`, sent first, checks out as the shared library and is copied into the internal flash's shared-library region |

An id is 1 to 32 letters, digits, `-` and `_`. Each app keeps its folder under `/apps/<id>/`:
`app.ntapp` is the compiled image, as `neotime-compiler` writes it, and `state` is its saved state.
The shared library is a single image that `neotime-compiler library -o shared.ntlib` writes.

## Where the code lives

- `services/companion`: reads lines and messages and writes replies, with no Bluetooth in it, so
  its tests run on a computer.
- `services/packages`: installs, lists and removes apps, and copies the shared library into
  internal flash, with no Bluetooth in it.
- `firmware/src/main.rs`: the GATT services, which hand lines to `services/companion` and send its
  replies.
- `services/settings`: each setting's limits and default, and the `/settings/system` file.
- `firmware/src/settings.rs` and `firmware/src/shell.rs`: apply settings and keep them on the
  external flash.
