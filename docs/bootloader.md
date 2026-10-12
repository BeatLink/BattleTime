# Bootloader

The bootloader is the first code that runs when the watch starts. It installs a waiting firmware
update or the recovery firmware, then starts the firmware. It is built on
[embassy-boot](https://embassy.dev/book/#_bootloader), so updates survive a power cut part way, and
a new firmware that fails to start is rolled back by itself.

It is about 15.7 KB, in a 16 KB region, which leaves about 300 bytes; its status screen takes
about 6 KB of that.

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
| `neotime-status-screen` | `drivers/status_screen/` | the watch, and in the simulator's tests | the small ST7789 driver for the blue status screen |
| `neotime-bootloader-updater` | `bootloader-updater/` | the watch only | the image that replaces this bootloader on a watch already on NeoTime |

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
| `SHARED_LIBRARY` | `0x69000` | 92 KB | images, fonts, components and common code that apps share; apps live on the external flash |

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
3. If the button is held, show "Keep holding for the menu" and keep watching it for 5 seconds. If
   it is still held, buzz once and open the menu (below).
4. Wake the external flash and check that it answers as a 4 MB chip. If it does not, skip
   straight to step 6 rather than leave the watch unable to start.
5. Let embassy-boot act on `BOOT_STATE`:
   - **Boot**: nothing to do.
   - **Swap**: show "Installing firmware, Do not unplug", then swap `FIRMWARE` with
     `FIRMWARE_UPDATE` page by page, recording progress in `BOOT_STATE`, so a power cut resumes
     where it stopped.
   - **Revert**: the last swapped-in firmware never marked itself booted, so swap the old one back.
6. Leave a note for the firmware in RAM (below), turn off every interrupt, then jump to the
   firmware at `FIRMWARE`.

Every flash read, write and erase feeds the watchdog, so a long swap does not trip it.

## The status screen

On an ordinary start the screen stays dark and the firmware starts at once. Whenever the bootloader
has work to do, it turns the screen blue (`status_colour::BOOTLOADER`) and shows the same page as
the firmware ([updates.md](updates.md#the-status-page)), titled "NeoTime bootloader", with
Bootloader `Working`, Filesystem `Not mounted` and Bluetooth `Off`. The screen code is its own
small ST7789 driver in `drivers/status_screen`, since the firmware's mipidsi-based driver does not
fit. It draws the 10x20 font straight from its bitmap rather than through embedded-graphics' text
code, which would not fit either. The simulator's tests run it against the simulator's ST7789
model, through the panel's SPI wires.

| When | Firmware / Action / Progress |
|---|---|
| The button is held at start | Unchanged / Keep holding for / the menu |
| The menu | Unchanged, then the three options from the action line down, the selected one marked with `>`, and "Press: next Hold: pick" |
| Rolling back | Swapping / Rolling back / Do not unplug |
| Installing the recovery firmware | Swapping / Installing recovery / Do not unplug |
| No previous firmware, or no valid recovery firmware | Unchanged / No previous firmware or No recovery found / Starting firmware |
| Swapping in an update, or rolling one back | Swapping / Installing firmware / Do not unplug |

The bootloader updater shows the page titled "NeoTime boot updater", with Firmware `Updater`, and
its Bootloader line goes `Checking`, `Writing`, then `Updated` or `Up to date`.

## The note to the firmware

The firmware cannot tell from `BOOT_STATE` alone that it was just installed as the recovery
firmware, so the bootloader leaves a note in RAM at `BOOT_NOTE_ADDRESS`, the third word of the
alive marker, which every program's linker keeps free. `BootNote::RecoveryInstalled` means the
button hold installed the recovery firmware on this start, and `BootNote::Normal` means anything
else. The top half of the word is a fixed pattern, so whatever RAM held at power-on never reads
as a note.

## Updating the bootloader

Nothing on the watch can replace the bootloader once it is installed, so a new one goes out inside
the bootloader updater (`bootloader-updater/`, built into `dist/bootloader-updater.zip` by
`tools/build_images.sh`). It is sent like any firmware update, and the bootloader swaps it in on
trial. It then:

1. Checks the bootloader it carries: its CRC, and a vector table that starts inside `BOOTLOADER`.
   A damaged one is never written; holding the button quits.
2. Compares it with the installed bootloader. If they match, nothing is written.
3. Waits for the charger, since a power cut during the write leaves nothing that starts. Holding
   the button meanwhile quits.
4. Erases and writes the bootloader's pages from RAM, then reads them back. On a mismatch it tries
   again, feeding the watchdog whatever the button does, and never resets with a broken bootloader.
5. Leaves a note at `UPDATER_NOTE_ADDRESS`, the alive marker's fourth word, and resets.

The updater never confirms itself, so its reset makes the new bootloader roll back to the firmware
that was running before. That firmware reads the note, clears the rollback's mark with
`neotime_updates::forget_rollback`, and its status page says "Bootloader updated" or "Bootloader up
to date" instead of "Update failed". Quitting leaves no note, so a quit reads as the failed update
it is. The write is about four pages, half a second or so, and is the only part with no way back;
[updates.md](updates.md#losing-power-during-the-bootloader-write) sets out what would have to go
wrong for a power cut to land in it.
Only a firmware from `db985cf` on reads the note. An older one shows the red "Update failed" page
after the hand-back even though the bootloader was written, as on the first watch; the Revert mark
it leaves clears with the next firmware update.

`tests/emulator/bootloader_update.robot` runs it from the bootloader the reloader put on the first
watch, kept in `tests/emulator/fixtures/` because it cannot be rebuilt bit for bit. On 2026-10-11
it replaced that bootloader on the watch itself: the rollback showed "Update failed", and the next
firmware update was swapped in under the new blue screen.

## What the firmware must do

- **Mark itself booted** with `neotime_updates::confirm_running_firmware` once it is sure it works,
  for example after the BLE stack and screen have started. Otherwise the next reset rolls it back,
  and no new update is accepted until it does. It only writes when the firmware is on trial, so
  after a rollback the Revert mark stays until the next update, and the status page can keep saying
  so.
- **Feed the watchdog.** It is still running when the firmware starts. Take it over with the
  same timeout, `WATCHDOG_TIMEOUT_IN_TICKS`, and feed it from the system loop.
- **Install updates** by feeding the new image, a raw `.bin` linked at `FIRMWARE`, to
  `neotime_updates::UpdateReceiver`, which stores it in `FIRMWARE_UPDATE` and marks the swap, then
  resetting.
- **Keep a recovery firmware** in `RECOVERY_FIRMWARE`, written with
  `neotime_updates::RecoveryReceiver`.

## The menu: rollback and recovery

The menu is for a firmware that starts but is broken badly enough that it cannot take an update,
or one that should be swapped for the firmware before it. It needs only the side button.

The system loop feeds the watchdog only while the button is up (see the button driver in
[architecture.md](architecture.md)), so the menu opens with one long hold:

1. Hold the button. After about 5 seconds the watchdog resets the watch.
2. Keep holding. After 5 more seconds in the bootloader (`MENU_BUTTON_HOLD_IN_MILLISECONDS`), the
   watch buzzes once and shows the menu. Let go.
3. A short press moves to the next option: **Start firmware**, **Roll back**, **Install recovery**.
   A press held for a second (`MENU_SELECT_HOLD_IN_MILLISECONDS`) picks the marked one, with a buzz.
   After 30 seconds without a press (`MENU_TIMEOUT_IN_MILLISECONDS`) it starts the firmware, so a
   watch is never left in the menu.
4. **Roll back** needs the firmware the last swap replaced, which sits one page up in the update
   slot (`PREVIOUS_FIRMWARE`). **Install recovery** needs `RECOVERY_FIRMWARE`. Either image must
   start with a stack pointer in RAM and a reset handler inside `FIRMWARE`; if it does not, the
   watch buzzes three times and starts the firmware it has, with nothing changed.
5. The bootloader marks the running firmware booted, which drops the progress a swap left in
   `BOOT_STATE`, then copies the image to the start of `FIRMWARE_UPDATE`
   (`copy_previous_into_update_slot` moves it down a page in place; `copy_recovery_into_update_slot`
   copies it across) and marks a swap. The image is then installed by the same power-cut-safe swap
   as any update, with the screen blue for up to about 30 seconds.

Clearing the state first matters for a firmware still on trial: its swap's progress is still in
`BOOT_STATE`, and marking a swap over it would leave that progress in place, so embassy-boot would
revert from the slot the copy has just overwritten.

A power cut during the copy loses the image in the update slot, but the running firmware is
untouched. One case is not covered: a power cut part way through a swap, then the menu, leaves a
half-swapped firmware for Roll back to replace, and Roll back then copies whatever the half swap
left above it; Install recovery recovers from that.

The firmware that was running ends up in the update slot. The installed firmware runs on trial and
must confirm itself, or the next reset swaps the other one back.

## Building and flashing

```sh
nix develop
cd bootloader
cargo build --release
```

`bootloader/` is not in the Cargo workspace, because it only builds for the watch.
`cargo run --release` flashes it over SWD with probe-rs.

## Not done yet

- **Signed updates.** embassy-boot's updater can check an ed25519 signature before marking a swap.
  That check runs in the firmware, in `neotime-updates`, so it costs the bootloader nothing; it is
  not used yet.
