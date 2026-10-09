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

## Firmware support

wasp-os branch `21-freeze-pkgmgr` carries the manager, raw transfer and face packages, and all
of it has run on a PineTime through wasp-os's own gatttool tools. Firmware older than that has
no manager, and every package command times out.

- **Raw transfer is untried from this app.** The driver sends raw bytes whenever the watch
  offers it, and the tests and the mock watch cover that path, but no Bluetooth path here has
  reached the watch yet; see above.

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
