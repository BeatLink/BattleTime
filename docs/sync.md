# Companion sync

The companion keeps a copy of every setting and record on the watch. While the watch is connected it
reads everything again on one interval, five minutes unless changed. The interval is set under
Settings, then Sync, in the companion. The same interval paces the companion's status reads (battery
and today's steps) in every build.

The Rust core does the syncing, so it runs in the desktop app now and on the phone once the phone
moves to Tauri. The code is in `companion/core/src/sync/`, and the desktop app's part is
`companion/tauri/src/sync.rs`.

## What is synced

Each pass reads the following. Each item is kept as one JSON file, named by its source, under
`sync/` in the app's data folder.

| Source | From the watch | Kept as |
|---|---|---|
| `settings` | the reply to a `settings` message ([companion-protocol.md](companion-protocol.md#settings-served)) | every setting by its key, the theme with its colours |
| `apps/<id>` | `/apps/<id>/state` and `/apps/<id>/disabled` | `{ "hidden": bool, "state": {...} }`, the state decoded as below, `null` for an app that keeps none |
| `files/settings/<name>` | every file in `/settings` | `quick_apps` as `{ "apps": [ids] }`, the rest as `{ "bytes": [...] }` |
| `steps/<year>/<MM>-<DD>` | that day's file | `{ "size": n, "slots": [...] }`, steps in each six-minute slot from midnight |
| `steps/trip` | `/steps/trip` | `{ "size": 4, "count": n }` |
| `logs/...` | every file under `/logs` | `{ "size": n, "bytes": [...] }` |

A past day's file is read again only when its size has changed. The newest day, loose counters and
everything else are read on every pass, since they change in place.

### App state

An app's saved state names each of its values, so the core decodes it without the app's image.
`runtime/src/state.rs` writes it:

- the file starts with `NTST`, version 1 and a count of values;
- each value then gives its key, its type, an array length (0 for a single value), an element width
  of 1, 2, 4 or 8 bytes, and the little-endian values.

The format is decoded into JSON as follows:

- a switch becomes `true` or `false`;
- times are milliseconds since 1970 in the watch's local time;
- durations are milliseconds;
- text is the string's number in the app's image;
- a record's fields, saved as `name.field`, come back together as one object.

For example, the alarm app's state reads as:

```json
{ "alarm_count": 2, "alarms": { "hour": [7, 9], "minute": [0, 30], "days": [62, 0], "enabled": [true, false] }, ... }
```

The watch writes an app's state when the app closes or the screen turns off. While an app is open,
its copy can be a little behind.

## Changing the watch from the companion

Changes go back the same two ways:

- **Settings** are sent as a `settings` message. The watch's reply is kept as the new `settings`.
- **App state** is changed in place. The core first reads the file again, then takes the new values,
  keeping each value's type, length and width so the app still loads it, and writes the file back. It
  then sends `{"t":"state","app":"<id>"}` ([companion-protocol.md](companion-protocol.md#app-state-served)).
  An open app then opens again on the new state rather than saving over it, and a closed alarm or
  timer notes its new time. A value that would not fit its width, or a list of the wrong length, is refused, and
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
