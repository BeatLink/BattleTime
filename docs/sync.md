# Companion sync

The companion keeps a copy of every setting and record on the watch. While the watch is connected it
reads everything again on one interval, five minutes unless changed. The interval is set under
Settings, then Sync, in the companion. The same interval paces the companion's status reads (battery
and today's steps), and the command line tool's.

The Rust core does the syncing, so it runs in the app, on the desktop and on Android alike. The
code is in `companion/core/src/sync/`, and the Tauri app's part is `companion/tauri/src/sync.rs`.

A phone and a desktop companion will keep this copy in step with each other too; see
[linked-companions.md](linked-companions.md).

## What is synced

Each pass reads the following. Each item is kept as one JSON file, named by its source, under
`sync/` in the app's data folder.

| Source | From the watch | Kept as |
|---|---|---|
| `settings` | the reply to a `settings` message ([companion-protocol.md](companion-protocol.md#settings-served)) | every setting by its key, the theme with its colours |
| `apps/<id>` | `/apps/<id>/state` and `/apps/<id>/disabled` | `{ "hidden": bool, "state": {...}, "kinds": {...} }`: the state decoded as below, `null` for an app that keeps none, and which keys hold a `time`, `duration`, `color` or `text` |
| `files/settings/<name>` | every file in `/settings` | `quick_apps` as `{ "apps": [ids] }`, the rest as `{ "bytes": [...] }` |
| `steps/<year>/<MM>-<DD>` | that day in its block, laid over its own file when older firmware left one | `{ "size": n, "slots": [...] }`, steps in each six-minute slot from midnight |
| `steps/trip` | `/steps/trip` | `{ "size": 4, "count": n }` |
| `logs/heart/<year>/<MM>-<DD>` | that day, as for steps | `{ "size": n, "slots": [...] }`, the average beats per minute in each six-minute slot from midnight, 0 for none |
| `logs/battery/<year>/<MM>-<DD>` | that day, as for steps | `{ "size": n, "slots": [...], "charging": [...] }`, the charge in percent at each slot's end, `null` while the watch was off, and whether it was charging |
| `logs/...` | every other file under `/logs` | `{ "size": n, "bytes": [...] }` |

| `day_blocks/<log>/<n>` | each block in `/day_blocks` | `{ "size": n }`, noting the block was read |

A day's size is that of its own file, ending after its last written slot. A past day's file is read
again only when its size has changed, and a block older than its log's two newest only when it has
not been read before or holds a day whose own file is laid under it. The newest day, the newest
blocks, loose counters and everything else are read on every pass, since they change in place.

### App state

An app's saved state names each of its values, so the core decodes it without the app's image.
`runtime/src/state.rs` writes it:

- the file starts with `NTST`, version 1 and a count of values;
- each value then gives its key, its type, an array length (0 for a single value), an element width
  of 1, 2, 4 or 8 bytes, and the little-endian values.

The format is decoded into JSON as follows:

- a switch becomes `true` or `false`;
- times are milliseconds since 1970 in UTC, and a time not set is `null`;
- durations are milliseconds;
- text is the string's number in the app's image;
- a record's fields, saved as `name.field`, come back together as one object.

The sports app keeps its last five cleared sessions this way, as `session_count` and a `sessions`
record of `started_at`, `duration` and `steps`, newest first.

For example, the alarm app's state reads as:

```json
{ "alarm_count": 2, "alarms": { "hour": [7, 9], "minute": [0, 30], "days": [62, 0], "enabled": [true, false] }, ... }
```

The watch writes an app's state when the app closes or the screen turns off. While an app is open,
its copy can be a little behind.

## Changing the watch from the companion

Changes go back the same two ways:

- **Settings** are sent as a `settings` message. The watch's reply is kept as the new `settings`.
- **App state** is changed in place. The core first sends `{"t":"state","app":"<id>","hold":true}`
  ([companion-protocol.md](companion-protocol.md#app-state-served)), so until it is done the watch
  neither runs that app while closed nor saves its state. It then reads the file again, takes the new
  values, keeping each value's type, length and width so the app still loads it, and writes the file
  back. Last it sends `{"t":"state","app":"<id>"}`, or `"hold":false` if anything failed, so the watch
  is never left holding. An open app then opens again on the new state rather than saving over it,
  and a closed alarm or timer notes its new time; one that came due during the hold rings then. A save
  the open app made during the hold is replaced by the core's write. The watch lets go by itself
  after 30 seconds or when the link drops. A value that would not fit its width, or a list of the wrong length, is refused, and
  nothing is written.

## Sharing the file service

The window has its own file client (`src/protocol/file-service.ts`) on the same characteristic as
the core, and the protocol's answers carry no tag. So the two take turns:

1. Before each operation, the window takes a lease with `files_begin`.
2. While it holds the lease, the file service's answers go to the window and the core waits. Without
   one, they go to the core.
3. A lease the window never ends is dropped after two minutes.

Both clients drop answers left over from an operation that gave up before starting the next.

## Plugins

A plugin reads the synced data and writes its own app's state through `neotime.data` (see
[plugins.md](plugins.md#synced-watch-data)), and hears `sync` events when its sources change.
