# Linked companions

!!! warning "A design, not built yet"
    This page is a design for work that has not started. None of it is in the companion today;
    the stages are in [companion/TODO.md](../companion/TODO.md#linked-companions).

The watch keeps one Bluetooth connection. When the wearer has the companion on a phone and on a
computer, the two companions find each other on the home network and link up. Only one of them
holds the watch; the other passes what it has through the one that does. The watch never knows
there are two.

The phone holds the watch away from home. At home it hands the watch to the desktop, and keeps
sending the desktop its calls, notifications, media and location for the watch. The companions
also keep each other's settings in step, so a change made in either shows in both.

All of it lives in the Rust core, in `companion/core/src/link/`, so the Tauri app on the desktop
and on Android share it.

## Who holds the watch

| Situation | Holder |
|---|---|
| The phone alone | the phone |
| The phone and a desktop linked | the desktop |
| The desktop alone | the desktop |

A handoff goes like this:

1. The phone and the desktop link up.
2. The phone tells the desktop it is letting go, disconnects from the watch, and stops reconnecting.
3. The watch advertises again, and the desktop connects to it.
4. The desktop tells the phone it has the watch.
5. If the desktop has not said so within 15 seconds, for example because the watch is in another
   room, the phone takes the watch back and tries again in 10 minutes.

The desktop cannot see the watch until the phone lets go, since the watch stops advertising while
connected, so the phone always moves first.

When the link drops, because the wearer left home, the network went down or the desktop went to
sleep, the phone goes back to reconnecting on its own. It gets the watch as soon as the desktop
loses it. While the phone holds the watch at home, the desktop does not try to connect.

## Finding and pairing

- **Finding:** each companion announces `_neotime._tcp` on the local network over mDNS, with its
  device id. A companion links only with the devices it has paired with.
- **Pairing, once:** the desktop shows a QR code holding its address, its public key and a
  one-time code. The phone scans it and sends its own public key with the code. Each keeps the
  other's key.
- **The link:** TLS with each side's self-made certificate, checked against the key kept at
  pairing, through the `rustls` the core already uses. Nothing else on the network can read the
  link or pose as either side.
- Either companion can unpair the other; that drops the link and forgets the key.

## Sync areas

What the companions keep in step is split into areas. Each area says where it is kept, who may
change it, and, for actions, where they run.

| Area | Kept on | Changed on | Notes |
|---|---|---|---|
| The watch's copy (`sync/`, see [sync.md](sync.md)) | every device | the holder only | the holder reads the watch and passes on what changed |
| Remote buttons | every device | any device | each button's action runs where it belongs; see below |
| Plugins: which are installed, and their settings | every device | any device | a plugin runs only on the holder |
| Notes, QR codes, quick apps, launcher, weather place | every device | any device | |
| Notification rules (which apps forward) | the phone | the phone | the desktop's own rules stay on the desktop |
| Bluetooth, keep-alive, sync interval | each device | that device | never sent |

Areas are declared in one table in the core. A new feature names its area and scope there and
gets syncing without code of its own.

### How records move

- Each area is a set of records by key, like the sources in the sync store.
- Each record carries a version: a timestamp joined with the id of the device that wrote it.
  The higher version wins, record by record. A removal is kept as a marker, so it spreads too.
- On linking, the two devices swap each area's newest versions and send what the other lacks.
  After that, each change is sent as it happens.
- A device never accepts a change to an area the table says it cannot change from there.

## Relaying

The holder's phone bridge gets a second source: the phone, over the link. On the desktop this
sits beside the computer's own D-Bus bridge.

| From the phone, to the watch | From the watch, to the phone |
|---|---|
| notifications, and their removal | a notification dismissed or answered |
| incoming and ended calls | answer, reject, mute |
| what is playing | media buttons |
| location | find my phone |
| Do Not Disturb | remote taps whose action runs on the phone |

When both sources send something:

- **Notifications** from both go to the watch, each tagged with its source, so dismissing one
  reaches the right device.
- **Calls** come only from the phone.
- **Media buttons** go to whichever device was playing last.
- **Time** comes from the holder.

## Where an action runs

A remote button keeps what it does on the companions, never on the watch, which sends only the
button's number ([README](../companion/README.md#remote)). The holder looks the button up in its
own copy and runs it, or sends it to the device it belongs to:

| Action | Runs on |
|---|---|
| `command` (a shell command) | the desktop it was made on |
| `android` (an intent, an app screen, a broadcast) | the phone |
| `builtin`: media | the device playing last |
| `builtin`: find my phone | the phone |
| `http` | the holder |

The device that runs an action looks it up in its own copy and runs only that, never a command
sent with the tap.

**A command from another device needs approval.** A `command` button that arrives by sync is kept
but switched off on that desktop until the wearer approves it there. Otherwise anyone who got hold
of the phone could run commands on the computer.

## Stages

1. **Finding and pairing:** mDNS, the QR code, and the TLS link, with a screen for linked devices.
2. **Sync areas:** the area table, record versions and the swap on linking, starting with remote
   buttons and the watch's copy.
3. **Handoff:** the phone lets go and the desktop takes the watch, with the fallback.
4. **Relaying:** the phone bridge's second source, and taps and replies sent back to the phone.
5. **The other areas:** plugins, notes and the rest, one at a time.

Each stage is tested in the core against two companions in the same process, with the fake watch.

## Not covered

- **iPhone.** iOS does not keep a network link open in the background reliably, and the Tauri
  app does not build for it.
- **Away from home.** The link is only over the local network; there is no server in between.
- **Two holders at once.** Exactly one device holds the watch. Several desktops can link, and the
  phone hands to whichever links first.
