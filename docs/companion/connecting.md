# Connecting a watch

The companion talks to the watch over Bluetooth Low Energy. There is no pairing code: the watch
accepts a connection from whichever phone or computer asks first, and keeps one connection at a
time.

## Finding the watch

1. Wake the watch. It advertises quickly for 30 seconds after it starts or after a phone leaves,
   then about once a second, so it can always be found, just more slowly after a while.
2. On the **Watch** screen, choose **Find a watch**. The list shows every NeoTime watch in range
   with its address and signal strength, and any watch waiting in a bootloader, marked
   **(bootloader)**.
3. Choose the watch. The companion connects, sets the watch's clock and time zone, and reads what
   the watch serves.

The watch advertises the name **NeoTime**. With **Show as InfiniTime** turned on in the watch's
settings it advertises **InfiniTime** instead, for Gadgetbridge; see
[Using Gadgetbridge instead](phone-features.md#using-gadgetbridge-instead).

The companion remembers the last watch. The Watch screen then offers **Reconnect to** it, and with
**Reconnect automatically** on in **Settings** it connects as soon as the app opens. **Forget
watch** in Settings clears it.

Only one companion should hold the watch at a time. Close the companion on one device before
connecting from another.

## What the Watch screen shows

Once connected, the Watch screen shows:

- **Status**: the battery charge and voltage, today's steps, and when the companion last synced,
  with **Sync now** to read everything again;
- **Now playing**, while music is forwarded, with Previous, Play or Pause and Next;
- **Diagnostics**: the watch's counters, read on request, and the **Debug report** card;
- **On the watch**: the screens for steps, alarms, watch settings, apps, plugins, quick apps,
  notes, QR codes, the launcher, the remote, files and backups;
- **Store**, which opens even with no watch connected;
- **Quick actions**: find the watch (it vibrates until tapped), send a test notification, one
  short buzz, and send the weather now.

A new watch with no face shows **This watch has no face yet** and a **Set up the watch** button;
[Apps, faces and the store](apps-and-store.md#setting-up-a-new-watch) describes it.

## What appears when

The companion offers only what the connected watch can do:

| The watch is | You get |
|---|---|
| Not connected | the Watch screen, the store and Settings |
| Waiting in its bootloader | firmware updates only; the Watch screen points to the Firmware tab |
| Running NeoTime | everything its services allow: files and apps through the file service, notifications, steps and alarms through the message channel |

A watch in its bootloader runs no firmware, so the companion can only send it a firmware package.
[Firmware updates](firmware-updates.md) says how.

## Connecting from a computer

The desktop app and the command line tool use the computer's Bluetooth through BlueZ. When the
computer has more than one adapter, they use one that is switched on.

The command line remembers the watch from the first `neotime scan` and connects to that address
later without scanning; `--address` or the `NEOTIME_ADDRESS` variable picks another. BlueZ forgets
an unpaired watch soon after it last saw it, so when it no longer knows the address the tool scans
until the watch appears, and says so if it never does.

## When the link drops

The watch drops a connection that has been idle for a while, and the app does not yet reconnect by
itself; choose **Reconnect** on the Watch screen. On the command line each command connects, does
its job and disconnects.

If a watch shows as connected but nothing reaches it, switching Bluetooth off and on clears it.
That drops every other Bluetooth device on the computer too.
