# Companion plugins

A plugin is the phone-side half of one watch app. It carries that app, installs it on the watch,
keeps the app's settings, and answers the app while the watch is connected. The companion's core
does only what every watch needs, and plugins add the rest at run time.

Plugins run in the desktop app today. The phone app gets them when it moves onto the same Rust core
(see the companion's `TODO.md`).

## What stays in the core

- finding, connecting to and reconnecting to the watch;
- firmware updates, and writing the time after one so NeoTime keeps it;
- the watch's own settings, files, backups and the raw console;
- installing, hiding and removing apps, and the launcher layout;
- the plugin manager: installing, turning on and off, and removing plugins;
- the phone's own sources (notifications, calls, media players, Do Not Disturb, location), which
  need native code on Android and iOS and so cannot arrive in a plugin.

## A plugin file

A plugin is one `.ntplugin` file, a zip holding:

```
manifest.json          who it is, what it carries and what it may do
watch/<id>.ntapp       the watch app, built by neotime-compiler (optional)
phone/main.js          the logic, run in the companion (optional)
phone/ui/index.html    the settings page, with any files beside it (optional)
```

The manifest:

```json
{
  "id": "buzz-back",
  "name": "Buzz back",
  "version": "1.0.0",
  "description": "A button on the watch that the companion answers by buzzing it back.",
  "watch": ["watch/buzz-back.ntapp"],
  "main": "phone/main.js",
  "ui": "phone/ui/index.html",
  "permissions": { "messages": ["vibrate"], "hosts": [] }
}
```

- `id` is 1 to 32 lowercase letters, digits and hyphens. It is also the watch app's id and the
  channel the app speaks on, so `send_to_phone("buzz-back", ...)` on the watch reaches this plugin.
- `permissions.messages` lists the messages the plugin may send the watch, from `notify`, `call`,
  `musicinfo`, `musicstate`, `weather`, `find` and `vibrate`. Settings, slots and the rest change
  the watch itself, so only the core sends those.
- `permissions.hosts` lists the hosts the plugin may fetch from, over https only.

The companion shows these permissions in words on the plugin's page, and refuses anything else.

## Building one

`companion/plugins/buzz-back` is a complete example. From `companion/`:

```bash
npm run build-plugin -- plugins/buzz-back
```

[scripts/build-plugin.mjs](../companion/scripts/build-plugin.mjs) reads the folder's `plugin.json`
(the manifest without file names), compiles `watch/app.tsx` with the watch's compiler, bundles
`phone/main.ts` with esbuild, copies `phone/ui/`, and writes `build/plugins/<id>.ntplugin`. In the
desktop app, Watch, then Plugins, then Install a plugin installs it and its watch app.

## The logic

`phone/main.js` runs in QuickJS inside the companion's Rust core, on a thread of its own, so it
keeps running while the window is closed. It is plain JavaScript; write it in TypeScript and let the
build script bundle it. It sees:

| | |
|---|---|
| `neotime.on("message", (d) => ...)` | The watch app sent `send_to_phone(id, d)`; `d` is its object of up to 4 numbers. |
| `neotime.on("connect", () => ...)`, `neotime.on("disconnect", ...)` | The watch connected or went away. A plugin started while it is connected hears `connect` at once. |
| `neotime.on("ui", (message) => ...)` | The settings page sent a message. |
| `neotime.send({ t: "vibrate", n: 2 })` | Sends the watch a message from `permissions.messages`, as [companion-protocol.md](companion-protocol.md) describes it; throws on any other. |
| `neotime.ui.send(message)` | Sends the settings page a message, when it is open. |
| `neotime.settings.get(key)`, `neotime.settings.set(key, value)` | The plugin's own settings, kept as JSON and saved after each event. |
| `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval` | Timers; an interval is at least a second. |
| `fetch(url, { method, headers, body })` | A request to a host in `permissions.hosts`, over https. Redirects come back unfollowed, and a reply is cut at 1 MB. |
| `console.log`, `console.warn`, `console.error` | Lines in the plugin's log, shown on its page. |

An error in a handler is logged and the plugin keeps running. Each event may run for 2 seconds and
the script may use 32 MB; past either, the script is stopped for that event. A script that fails to
load leaves the plugin installed but stopped.

## The settings page

`phone/ui/index.html` opens in a sandboxed frame on the plugin's page, served from
`plugin://<id>/`. It may run scripts and load its own files, and nothing else. It talks to the
plugin with messages wrapped as `{ neotime: message }`:

```js
parent.postMessage({ neotime: { buzzes: 3 } }, "*");   // reaches neotime.on("ui", ...)
window.addEventListener("message", (event) => {
  const message = event.data && event.data.neotime;    // from neotime.ui.send(...)
});
```

## Where the code lives

| | |
|---|---|
| `companion/core/src/plugins/` | Manifest, package reader, store on disk, QuickJS runtime, HTTP, and the host that joins them to the watch |
| `companion/core/src/gadgetbridge.rs` | The watch's message lines, shared with the host |
| `companion/tauri/src/plugins.rs` | The desktop app's commands, its link from the host to the watch and window, and the `plugin://` pages |
| `companion/src/app/plugins.tsx`, `src/services/plugins.ts` | The Plugins screen |
| `companion/core/examples/plugin_host.rs` | Installs a plugin into a folder and runs it against a real watch from the terminal |

Installed plugins live in the desktop app's data folder, under `plugins/<id>/`, beside their
on/off state and settings.
