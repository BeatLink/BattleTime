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
| Current Time | `0x1805` | `0x2A2B`, read and write | Writing the 10-byte Current Time value sets the clock. The watch keeps local time, so write local time. | Yes |
| Battery | `0x180F` | `0x2A19`, read and notify | The charge, 0 to 100. | Not yet |

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

An unknown preset, a colour not written `#rrggbb` or a brightness outside 1 to 3 is refused with
an `error` reply, and nothing in that message is applied.

The reply:

```json
{"t":"settings","theme":"dark","theme_colors":{"background":"#000000","surface":"#292c31",
 "text":"#ffffff","muted":"#8c929c","accent":"#00aaa5","success":"#39be5a","warning":"#f7aa18",
 "danger":"#e74539"},"themes":["dark","light","ocean","forest","sunset","contrast"],"brightness":2}
```

`theme` is the preset whose colours match exactly, or `custom`. Colours are stored as RGB565, so a
colour reads back to the nearest one the screen shows. The watch keeps the theme in
`/settings/theme` and the brightness in `/settings/brightness`, so they survive a restart.

### Other messages (not served yet)

These follow Gadgetbridge's Bangle.js messages, so its driver works too.

Phone to watch:

| `t` | Fields | Meaning |
|---|---|---|
| `notify` | `id`, `src`, `title`, `body` | A notification arrives. |
| `notify-` | `id` | A notification is dismissed. |
| `call` | `cmd` (`incoming`, `start`, `end`, ...), `name`, `number` | A call. |
| `musicinfo` | `artist`, `album`, `track`, `dur` | The track playing. |
| `musicstate` | `state` (`play`, `pause`), `position` | The player's state. |
| `weather` | `temp` (kelvin), `hum`, `code`, `txt`, `wind`, `wdir`, `loc` | The weather, which apps read from `current_weather`. |
| `find` | `n` (bool) | Start or stop ringing the watch. |
| `vibrate` | `n` (1 to 5) | Buzz. |
| `alarm` | `d`: a list of `{h, m, on, rep}` | Set the alarms; with no `d`, report them. |
| `status` | | The watch replies `{"t":"status","bat":...,"mv":...,"steps":...,"charging":...}`. |
| `steps` | `y`, `m`, `d` | The watch replies with that day's steps in 240 six-minute slots, `null` where unknown. |
| `http` | `id`, `resp` or `err` | The answer to a watch's `http` request. |

Watch to phone: `music` with `n` (`play`, `pause`, `next`, `previous`, `volumeup`,
`volumedown`); `call` with `n` (`ACCEPT`, `REJECT`, `IGNORE`); `findPhone` with `n`; `http` with
`id`, `url`, `method`, `body`; `info` and `error` with `msg`.

## The NeoTime service

Service `6e540001-0000-4000-8000-4e656f54696d`, with its characteristics on the same base:

| Characteristic | UUID | Use | Served |
|---|---|---|---|
| Diagnostics | `6e540002-…` | Read: eight little-endian `u32` counters; see [architecture.md](architecture.md). | Yes |
| Packages | `6e540003-…` | Write and notify: one JSON command per line, as in [Packages](#packages-not-served-yet). | Not yet |
| File data | `6e540004-…` | Write without response and notify: raw file bytes. | Not yet |

### Files (not served yet)

Paths are absolute, such as `/apps/stopwatch/state`. Each command is a JSON line written to file
control, and each answer a JSON line notified on it; a failure answers with the command and
`"err":"..."`.

| Command | Answer |
|---|---|
| `{"c":"ls","p":path}` | `{"c":"ls","e":[[name,size,is_folder],...]}`, `is_folder` as 0 or 1 |
| `{"c":"get","p":path}` | `{"c":"get","n":size}`, then the bytes notified on file data |
| `{"c":"put","p":path,"n":size}` | `{"c":"put","ok":1}`; the phone then writes the bytes to file data, in pieces of at most MTU − 3, and the watch notifies `{"c":"put","at":offset}` every 4 KB and `{"c":"put","done":1}` at the end |
| `{"c":"rm","p":path}` | `{"c":"rm","ok":1}`; a folder must be empty |
| `{"c":"df"}` | `{"c":"df","free":bytes,"total":bytes}` |
| `{"c":"install","id":id}` | `{"c":"install","ok":1}` once `/apps/<id>/app.ntapp` checks out and the app appears in `installed_apps` |
| `{"c":"remove","id":id}` | `{"c":"remove","ok":1}`; deletes `/apps/<id>` with its saved state |

Each app keeps its folder under `/apps/<id>/`: `app.ntapp` is the compiled image, as
`neotime-compiler` writes it, and `state` is its saved state.

### Packages (not served yet)

An app or face is sent as a file first, to `/apps/<id>/app.ntapp`, then installed with a command on
the packages characteristic. Each command is a JSON line written to it, and each answer a JSON line
notified on it, in pieces of up to 20 bytes; a failure answers with the command and `"err":"..."`.

| Command | Answer |
|---|---|
| `{"c":"apps"}` | `{"c":"apps","a":[[id,name,kind,enabled,bytes],...]}`, `kind` being `app` or `face` and `enabled` 0 or 1, in id order |
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
- `firmware/src/settings.rs` and `firmware/src/shell.rs`: apply settings and keep them on the
  external flash.
