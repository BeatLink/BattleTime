# Plugins (draft)

The companion is moving to a small core plus plugins. Each plugin is the phone-side half of one
watch app: it carries that app, installs it, and owns its settings and its traffic. Nothing here is
built yet; this is the shape the Rust core is being built towards.

## What stays in the core

The core does only what every watch needs, whatever is installed on it:

- finding, connecting to and reconnecting to the watch;
- firmware updates, and writing the time after one so NeoTime keeps it;
- the watch's own settings, files, backups and the raw console;
- installing, updating, hiding and removing apps, the shared library and the launcher layout;
- the plugin manager: installing a plugin from a file or a link, updating it, turning it off and
  removing it, together with its watch app;
- the phone's own sources (notifications, calls, media players, Do Not Disturb, location), which
  need native code on Android and iOS and so cannot arrive in a plugin. Plugins reach them through
  the host API, if the user allows it.

## What a plugin is

A plugin is one `.ntplugin` file, a zip holding:

```
manifest.json      id, name, version, the firmware versions it works with, its permissions
watch/*.ntapp      the watch app or apps it carries, built by neotime-compiler
phone/main.js      its logic, compiled from TypeScript
phone/ui/          its settings screen: an HTML page and its scripts
```

Its `id` is its watch app's id, so the app's files sit under `/apps/<id>/` on the watch.

### Logic

`phone/main.js` runs in QuickJS inside the Rust core, not in the window. That way it keeps
running when the window is closed and the app is in the background on a phone, and it runs on iOS,
which allows no JIT. Authors write TypeScript, the same language as the watch apps.

The host API it gets:

- messages from its own watch app, and a way to send messages back;
- files under its own `/apps/<id>/` on the watch, and nowhere else;
- its own stored settings;
- timers that run while the watch is connected;
- fetching from the hosts its manifest names;
- with permission: notifications, media players, location, and, on a computer, running a command.

### Settings screen

`phone/ui/` is shown in a sandboxed frame inside the core's window. It talks only to its own
logic, by messages through the core, so it has no more reach than the logic has.

### Permissions

The manifest names every permission and every host; the core shows them at install and grants
nothing else. A plugin cannot touch another plugin's files or settings.

## What becomes a plugin

These leave the core and become plugins, each with the watch app it already pairs with: weather,
notes, remote, alarms, steps, music, navigation and notification forwarding. The watch's own
settings, files, backup and console stay in the core.

## Open questions

- Where plugins come from: only files and links, or also a list the core can browse.
- Whether plugins are signed, and what the core does with an unsigned one.
- Whether a plugin may carry a watch face as well as an app.
