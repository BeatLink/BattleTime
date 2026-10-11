# Bootloader

The bootloader is the first code that runs when the watch starts. It installs a waiting firmware
update or the recovery firmware, then starts the firmware. It is built on
[embassy-boot](https://embassy.dev/book/#_bootloader), so updates survive a power cut part way, and
a new firmware that fails to start is rolled back by itself.

It is about 8 KB, in a 16 KB region. The rest is room for signature checks later.

The work of an update is split three ways:

- **The bootloader** only acts on the watch: it swaps, rolls back and installs recovery.
- **The firmware's Bluetooth** only uploads: it writes the new image into `FIRMWARE_UPDATE`, marks it,
  and resets. It will speak Nordic's legacy DFU, as InfiniTime does, so Gadgetbridge and the other
  companion apps can update NeoTime unchanged.
- **A reloader image** moves a watch onto NeoTime once, from InfiniTime or wasp-os (below).

## Crates

| Crate | Path | Runs on | What it holds |
|---|---|---|---|
| `neotime-bootloader` | `bootloader/` | the watch only | the bootloader program |
| `neotime-boot` | `boot/` | the watch and in tests | the flash layout, the watchdog timeout, the recovery copy |
| `neotime-external-flash` | `drivers/external_flash/` | the watch and in tests | the external flash driver |

`neotime-boot` is shared with the firmware, which must use the same layout and watchdog timeout.
The bootloader's linker memory map is generated from it by `bootloader/build.rs`, so the two
cannot drift apart. The layout rules embassy-boot checks at start-up are also checked when
`neotime-boot` compiles, so a bad layout fails the build instead of the watch.

## Flash layout

The firmware runs from internal flash, because the chip cannot run code straight from the external
flash. Updates and the recovery firmware wait in external flash until they are swapped in.

Internal flash, 512 KB:

| Region | Start | Size | Holds |
|---|---|---|---|
| `BOOTLOADER` | `0x00000` | 16 KB | this bootloader, the first code to run; there is no Nordic MBR or SoftDevice |
| `BOOT_STATE` | `0x04000` | 4 KB | embassy-boot's record of what to do at the next start |
| `FIRMWARE` | `0x05000` | 400 KB | the running firmware, which is linked to start here |
| `INSTALLED_APPS` | `0x69000` | 92 KB | kept for apps installed over BLE; the app format is still open |

External flash, 4 MB:

| Region | Start | Size | Holds |
|---|---|---|---|
| `FIRMWARE_UPDATE` | `0x000000` | 404 KB | a new firmware waiting to be installed; one page more than `FIRMWARE`, which the swap needs |
| `RECOVERY_FIRMWARE` | `0x065000` | 400 KB | a small firmware that can only receive a new firmware over BLE |
| `RESERVED_FOR_GROWTH` | `0x0C9000` | 220 KB | lets the two slots grow without moving the file system |
| `FILE_SYSTEM` | `0x100000` | 3 MB | littlefs: apps, settings and shared resources (see [storage.md](storage.md)) |

Growing `FIRMWARE` later needs a new bootloader, since the bootloader is built with these numbers.

## What happens at start

1. Make sure the NFC pins are plain GPIO, since P0.10 resets the touch panel. If an erased UICR
   has turned NFC back on, embassy-nrf turns it off and resets the watch.
2. Start the watchdog with a 5 second timeout. If the firmware left it running with a different
   timeout, it cannot be changed, so wait for it to reset the watch.
3. If the button is held, keep watching it for 5 seconds. If it is still held, buzz once and
   install the recovery firmware (below).
4. Wake the external flash and check that it answers as a 4 MB chip. If it does not, skip
   straight to step 6 rather than leave the watch unable to start.
5. Let embassy-boot act on `BOOT_STATE`:
   - **Boot**: nothing to do.
   - **Swap**: swap `FIRMWARE` with `FIRMWARE_UPDATE` page by page, recording progress in
     `BOOT_STATE`, so a power cut resumes where it stopped.
   - **Revert**: the last swapped-in firmware never marked itself booted, so swap the old one back.
6. Turn off every interrupt, then jump to the firmware at `FIRMWARE`.

Every flash read, write and erase feeds the watchdog, so a long swap does not trip it.

## What the firmware must do

- **Mark itself booted** with `neotime_updates::confirm_running_firmware` once it is sure it works,
  for example after the BLE stack and screen have started. Otherwise the next reset rolls it back,
  and no new update is accepted until it does.
- **Feed the watchdog.** It is still running when the firmware starts. Take it over with the
  same timeout, `WATCHDOG_TIMEOUT_IN_TICKS`, and feed it from the system loop.
- **Install updates** by feeding the new image, a raw `.bin` linked at `FIRMWARE`, to
  `neotime_updates::UpdateReceiver`, which stores it in `FIRMWARE_UPDATE` and marks the swap, then
  resetting.
- **Keep a recovery firmware** in `RECOVERY_FIRMWARE`, written with
  `neotime_updates::RecoveryReceiver`.

## Recovery

Recovery is for a firmware that starts but is broken badly enough that it cannot take an update.

The system loop feeds the watchdog only while the button is up (see the button driver in
[architecture.md](architecture.md)), so the whole gesture is one long hold:

1. Hold the button. After about 5 seconds the watchdog resets the watch.
2. Keep holding. After 5 more seconds in the bootloader, the watch buzzes once. Let go.
3. The bootloader checks that `RECOVERY_FIRMWARE` holds a firmware: its first two words must be a
   stack pointer in RAM and a reset handler inside `FIRMWARE`. If they are not, the watch buzzes
   three times and starts the old firmware.
4. It copies the recovery firmware into `FIRMWARE_UPDATE` and marks a swap, so the recovery firmware
   is installed by the same power-cut-safe swap as any update. The screen stays dark meanwhile,
   for up to about 30 seconds.

The broken firmware ends up in `FIRMWARE_UPDATE`. The recovery firmware must mark itself booted,
or the next reset swaps the broken firmware back in.

## Building and flashing

```sh
nix develop
cd bootloader
cargo build --release
```

`bootloader/` is not in the Cargo workspace, because it only builds for the watch.
`cargo run --release` flashes it over SWD with probe-rs.

## Not done yet

- **Never run on a watch.** It builds and the shared code is tested on the computer, but nothing
  has been tried on hardware.
- **The reloader.** A watch running InfiniTime or wasp-os takes one special update, the reloader,
  through that firmware's own update path. The reloader writes this bootloader to address 0 and
  NeoTime to `FIRMWARE`, replacing MCUBoot or wasp-os's MBR, SoftDevice and bootloader. It is being
  written against `neotime-boot`'s layout, and can be tried first in the Renode emulator in
  `PineTimeEmulator/`. Rewriting address 0 is the one step with no way back: a power cut or a bug
  there leaves the watch unable to start until it is reflashed over SWD.
- **The recovery firmware** itself.
- **Signed updates.** embassy-boot's updater can check an ed25519 signature before marking a swap.
  That check runs in the firmware, in `neotime-updates`, so it costs the bootloader nothing; it is
  not used yet.
