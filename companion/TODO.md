# Open items

What is known to be unfinished in this app. Features live in the watch repo's
`ROADMAP.md`, and the app package design and its task list live in that repo's
`docs/app-packaging-design.md`; only this app's own loose ends are here.

## Firmware support

wasp-os branch `21-freeze-pkgmgr` carries the manager, raw transfer and face packages. The
command line tool has installed, enabled and removed a package on a PineTime over raw transfer.
Firmware older than that has no manager, and every package command times out.

- **The phone and desktop builds are untried on a watch.** They share the protocol code the
  command line tool ran, but not its Bluetooth layer on a phone, and the desktop app has not been
  opened against the watch.

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
