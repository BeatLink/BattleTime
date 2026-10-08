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

## Waiting on firmware

- **Raw transfer stays unavailable** until the board enables `MICROPY_PY_SYS_STDIO_BUFFER`. The
  watch reports `raw: false`, so every package install goes over base64 with its third of extra
  bytes. The driver already picks the fast path the moment the watch offers it.
- **Transfer windows stay at 96 bytes** until the receive ring in the firmware's `ble_uart.c`
  grows from 128 bytes. The driver honours whatever window the watch reports.
- **Watch faces cannot be installed as packages** until the firmware's Faces app reads the package
  index. The app already builds and sends face packages.

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
