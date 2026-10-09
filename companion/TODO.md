# Open items

What is known to be unfinished in this app. Features live in the watch repo's
`ROADMAP.md`, and the app package design and its task list live in that repo's
`docs/app-packaging-design.md`; only this app's own loose ends are here.

## Blocked on the watch, not on this code

- **The desktop and command line Bluetooth paths are unverified against real hardware.** BlueZ
  reports the link to the PineTime here as connected and then never resolves its services, so
  every D-Bus client stalls in service discovery, node-ble and `bluetoothctl` alike. `gatttool`
  with `-t random` reaches the same watch, which points at BlueZ rather than at this code. Both
  paths now time out each step and say where they stalled, so the failure is legible. Retry on a
  watch that BlueZ can serve, or clear the wedged adapter, before trusting the transport. See the
  known limitation in [README.md](README.md).

## Waiting on a watch running the new firmware

The firmware side is finished on wasp-os branch `21-freeze-pkgmgr`, but none of it has run on a
watch yet. A watch on older firmware still has no manager to answer, and every package command
times out.

- **Package commands** need that firmware, which freezes the manager and puts `pkg` on the REPL.
- **Raw transfer and 512 byte windows** come with it, through `sys.stdin.buffer` and a 1 KB
  receive ring. Older firmware reports `raw: false` and 96 byte windows, and the driver follows
  whatever the watch reports.
- **Watch face packages** appear in the firmware's Faces app once installed and enabled.

## Not started

- **A package repository.** The app only installs what it ships. A repository needs an index
  format, hosting, an update story and a decision on whether packages are signed.
- **Installing a package from the phone's own files**, picked with the system file picker. Little
  work, and the natural loop for someone developing their own watch app.

Both reuse the existing ABI check and transfer driver, so only the source of the bundle changes.

## Notification forwarding

Reading phone notifications still needs native platform code; see
[src/services/notifications/README.md](src/services/notifications/README.md). Media control from
the watch needs a media session bridge on each platform.
