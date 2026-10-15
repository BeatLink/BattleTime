# BattleTime

BattleTime is a firmware for the PineTime smartwatch, written in Rust.

It aims to combine the best parts of the two existing PineTime firmwares:

- from [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime): a polished UI, long battery
  life, speed, and wide companion app support through standard BLE services;
- from [wasp-os](https://github.com/wasp-os/wasp-os): apps that are easy to write and install over
  Bluetooth without reflashing, a desktop simulator, and a live console for development.

## Goals

- **Apps in Preact, no Rust needed.** Apps and watch faces are written as
  [Preact](https://preactjs.com) components in TypeScript, with
  [Signals](https://preactjs.com/guide/v10/signals/) for state, and previewed in a browser. The
  compiler turns them into compact bytecode for the watch, so writing an app takes web skills,
  not Rust or embedded knowledge.
- **Loadable apps.** Apps and watch faces install over BLE, with no reflash.
- **Apps built from shared parts.** Widgets, functions, images and fonts that apps have in common
  live in the firmware and its shared library, and apps assemble them, so each app is small, easy
  to write and light on RAM, the scarcest resource on the watch.
- **Pure Rust.** The whole firmware is Rust, including the Bluetooth stack: the
  [TrouBLE](https://github.com/embassy-rs/trouble) host on BattleTime's own radio controller,
  built on [Embassy](https://embassy.dev).
- **Safe updates.** A bad firmware image rolls back by itself.
- **Battery life.** The watch sleeps whenever it can.
- **Companion apps.** It works with Gadgetbridge and the other apps InfiniTime supports, and has a
  companion app of its own.

## Status

Early work. The drivers, the bootloader, a firmware with a watch face, a launcher and apps, and the
reloader all run on a PineTime. On 2026-10-11 the reloader moved a watch from wasp-os onto BattleTime,
and firmware updates over Bluetooth and the button-hold recovery have worked on it since. Moving a
watch from InfiniTime has run only in the emulator.

## This repository

- [battletime/](battletime/) is the watch: the firmware, bootloader, apps, faces, simulator and their
  tests. Run its commands from inside it.
- [companion/](companion/) is the BattleTime Companion, the phone and desktop app that installs apps,
  updates the firmware and forwards notifications. See [its README](companion/README.md).
- [docs/](docs/) is the documentation for both, also published as the website.
- [tools/](tools/) builds the watch images, packages firmware updates and measures memory use.
  [tools/appbuilder/](tools/appbuilder/) is the App Builder, a VS Code extension that builds apps
  from a palette of widgets, events and effects with the simulated watch beside the code.

`nix develop` gives the watch's toolchain and `nix develop .#companion` the companion's; both work
from anywhere in the repository.

To try the firmware without a watch, run it in PineTimeEmulator, as
[testing.md](docs/testing.md#trying-battletime-in-the-emulator) describes.

## Documentation

The docs are also on the website, <https://beatlink.github.io/BattleTime/>, in four parts.

**Using the watch** — start at [the guide](docs/guide/index.md).

- [Getting around](docs/guide/getting-around.md), [screen and battery](docs/guide/screen-and-battery.md),
  [time and alarms](docs/guide/time-and-alarms.md),
  [notifications and calls](docs/guide/notifications-and-calls.md),
  [health and fitness](docs/guide/health-and-fitness.md),
  [phone and Bluetooth](docs/guide/phone-and-bluetooth.md),
  [apps and faces](docs/guide/apps-and-faces.md) and
  [updates and recovery](docs/guide/updates-and-recovery.md).
- [All apps](docs/apps.md), each with its own page, and [watch faces](docs/faces.md).

**The companion app** — start at [the companion guide](docs/companion/index.md): connecting a
watch, installing apps, phone features, watch data, firmware updates and the command line.

**Writing apps** — start at [writing apps](docs/writing-apps.md), then the
[app reference](docs/app-reference.md), [testing apps](docs/testing.md),
[companion plugins](docs/plugins.md) and the [app store](docs/app-store.md).

**How it is built** — start at [the architecture](docs/architecture.md), which leads to the
drivers, services, firmware, bootloader, Bluetooth, storage and [memory map](docs/memory-map.md)
pages. Open work is in [to do](docs/todo.md).

## Credits

BattleTime stands on the work of these projects and the people behind them.

**The watch firmwares it grew from**

- [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime): the user interface, watch faces,
  apps, Bluetooth services and icon artwork BattleTime follows.
- [wasp-os](https://github.com/wasp-os/wasp-os): loadable apps, the desktop simulator, and its
  apps and faces.
- [PinetimeLite](https://github.com/joaquimorg/PinetimeLite): its watch faces.
- [ZephyrLabs' Watchfaces](https://github.com/ZephyrLabs/Watchfaces), by ZephyrLabs and jlukanc1:
  the faces from its collection.

**What the watch is built on**

- [Embassy](https://embassy.dev): the async executor, the nRF52 drivers and the bootloader's update
  logic.
- [TrouBLE](https://github.com/embassy-rs/trouble): the Bluetooth host.
- [rlvgl](https://github.com/softoboros/rlvgl) and [LVGL](https://lvgl.io): the drawing and the
  widget set.
- [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics): the simulator's
  display.
- [Preact](https://preactjs.com) and [Signals](https://preactjs.com/guide/v10/signals/): the
  language apps are written in.
- [Font Awesome](https://fontawesome.com) and
  [Material Icons](https://github.com/google/material-design-icons): the icons.

**The companion, tools and website**

- [Tauri](https://tauri.app), [btleplug](https://github.com/deviceplug/btleplug) and
  [QuickJS](https://bellard.org/quickjs/) through [rquickjs](https://github.com/DelSkayn/rquickjs):
  the companion app and its plugins.
- [Gadgetbridge](https://gadgetbridge.org): the phone app whose protocol the watch speaks.
- [InfiniEmu](https://github.com/pipe01/InfiniEmu): the emulator that
  [PineTimeEmulator](https://github.com/BeatLink/PineTimeEmulator) is built from.
- [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/): the website.
- [PINE64](https://www.pine64.org/pinetime/): the PineTime itself.

## License

BattleTime is free software under the GNU General Public License, version 3 or (at your option)
any later version. The full text is in [LICENSE](LICENSE).
