# NeoTime

NeoTime is a firmware for the PineTime smartwatch, written in Rust.

It aims to combine the best parts of the two existing PineTime firmwares:

- from [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime): a polished UI, long battery
  life, safe updates with rollback, and wide companion app support through standard BLE services;
- from [wasp-os](https://github.com/wasp-os/wasp-os): apps that are easy to write and install over
  Bluetooth without reflashing, a desktop simulator, and a live console for development.

## Goals

- **Loadable apps.** Apps and watch faces install over BLE, with no reflash.
- **Pure Rust.** The whole firmware is Rust, including the Bluetooth stack: the
  [TrouBLE](https://github.com/embassy-rs/trouble) host on NeoTime's own radio controller,
  built on [Embassy](https://embassy.dev).
- **Safe updates.** A bad firmware image rolls back by itself.
- **Battery life.** The watch sleeps whenever it can.
- **Companion apps.** It works with Gadgetbridge and the other apps InfiniTime supports.

## Status

Early work. The drivers, the bootloader and a first firmware that takes updates over Bluetooth
exist, and all have run only on the computer. Nothing has run on a watch yet.

## Documentation

- [Architecture](docs/architecture.md): how the code is laid out and how the parts fit together.
- [Coding guidelines](docs/coding-guidelines.md): how NeoTime code is written.
- [Simulator](docs/simulator.md): running the drivers in a desktop window with `nix run`.
- [To do](docs/todo.md): work that is known to be needed but not done yet.
- [Memory map](docs/memory-map.md): how the RAM, internal flash and external flash are divided.
- [Updates](docs/updates.md): firmware updates over Bluetooth.

## License

NeoTime is free software under the GNU General Public License, version 3 or (at your option)
any later version. The full text is in [LICENSE](LICENSE).
