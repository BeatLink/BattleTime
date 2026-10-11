# Firmware updates

How a new firmware gets onto the watch, and how a watch gets onto NeoTime in the first place.
Three pieces share the work:

| Piece | Where | Does |
|---|---|---|
| Bluetooth upload | `services/dfu`, in the firmware | takes the image from the phone and hands it to the receivers |
| Receivers | `services/updates`, in the firmware | store the image in the right slot and mark it for the bootloader |
| Bootloader | `bootloader/` | swaps the image in at the next start and rolls back a bad one |
| Reloader | `reloader/` | a one-time image that moves a watch from InfiniTime or wasp-os onto NeoTime |

[bootloader.md](bootloader.md) covers the bootloader and the flash layout. This page covers the
rest.

## The protocol: Nordic legacy DFU

NeoTime speaks the same update protocol as InfiniTime: Nordic's legacy DFU, the one the nRF5 SDK
bootloaders spoke before secure DFU. Gadgetbridge, ITD, Siglo and NeoTime's own tools all have it,
so they update NeoTime without changes.

Three characteristics under the service `00001530-1212-efde-1523-785feabcd123`:

| Characteristic | UUID ends in | The phone | The watch |
|---|---|---|---|
| Control point | `1531` | writes a command | answers with a notification |
| Packet | `1532` | writes data, 20 bytes at a time, no response | |
| Version | `1534` | reads it | answers `08 00`, as the Nordic bootloader does |

One update, as the companion app and Gadgetbridge drive it:

1. `START_DFU` (1) with the image type, then on the packet characteristic three 4-byte sizes:
   SoftDevice, bootloader, application. Only the application size may be non-zero. The watch
   erases the slot and answers success.
2. `INITIALIZE_DFU` (2) with 0, the init packet on the packet characteristic, then
   `INITIALIZE_DFU` with 1. The init packet is Nordic's `.dat`: device type, revision, version, a
   list of SoftDevice IDs, and a CRC-16 of the image. The watch keeps the CRC and ignores the rest.
3. Optionally `SET_RECEIPT_INTERVAL` (8) with a count, so the watch reports progress every that
   many packets.
4. `RECEIVE_FIRMWARE_IMAGE` (3), then the image on the packet characteristic. The watch collects
   packets into 240-byte pieces before writing them to flash, sends a receipt (17) every interval,
   and answers success when the last byte is in.
5. `VALIDATE_FIRMWARE` (4). The watch reads the whole slot back, computes the CRC-16 and compares
   it with the init packet's. A mismatch answers `CRC_ERROR` and forgets the upload.
6. `ACTIVATE_IMAGE_AND_RESET` (5). The watch checks the image is whole and starts with a firmware
   vector table, marks it for the bootloader, and resets. The bootloader swaps it in.

Responses are three bytes: 16, the command, and a result (1 success, 2 wrong state, 3 not
supported, 4 too large, 5 CRC error, 6 failed). A command out of turn gets "wrong state"; an
image larger than the slot gets "too large" at step 1; a SoftDevice or bootloader image gets "not
supported".

NeoTime's one addition is an image type for the recovery slot: `START_DFU` with type `0x08` (the
standard types are 1, 2 and 4) sends the image to `RECOVERY_FIRMWARE` instead of the update slot.
Companion apps never send it; `tools/` will.

`neotime-dfu` is the state machine, with no Bluetooth in it, so its tests drive a whole update
through it against fake receivers. The `trouble` feature adds the GATT service definition the
firmware registers with TrouBLE.

### Packaging an image

`tools/package_dfu.py firmware.bin firmware.zip` makes the zip the clients expect: a manifest, the
init packet with the CRC-16, and the image. The companion app and Gadgetbridge open it as they
would an InfiniTime release.

## The receivers

`neotime-updates` is the on-watch half, with no protocol in it:

- **`UpdateReceiver`** takes an image with `begin(size)`, `receive(chunk)` in order, and
  `finish()`, writing through embassy-boot's updater into the update slot and erasing each sector
  just before it is first written. `finish` checks the size and the vector table and marks the
  swap. `read_back` serves the protocol's CRC check.
- **`RecoveryReceiver`** does the same into the recovery slot. It erases the first sector before
  anything else and writes the 8-byte vector table last, so a half-written image never passes for
  a whole one when the bootloader looks.
- **`confirm_running_firmware`** marks the running firmware as working. Until it is called the
  bootloader rolls the firmware back at the next reset, and no new update is accepted.
- **`Receivers`** holds both behind the `UpdateTargets` trait, which the protocol drives with an
  `ImageKind`.

## The firmware's part

The first firmware (`firmware/`) does the little an update needs and not much more:

- takes over the bootloader's watchdog with the same settings and feeds it only while the side
  button is up, so holding the button for about five seconds always gets back to the bootloader;
- advertises as "NeoTime" with the DFU service, and once advertising works calls
  `confirm_running_firmware`, so a firmware whose Bluetooth is broken rolls back by itself;
- serves the DFU service on every connection, shows progress on the screen, and resets when the
  protocol asks;
- mounts the file system, so that an update which breaks storage is noticed on the screen;
- writes an alive marker into the last words of RAM, which an emulator checks.

It is 105 KB of flash and 25 KB of static RAM, 16 KB of that heap. Its screen writes and flash
writes are blocking, so the Bluetooth host waits while a flash sector erases; the link layer runs in
an interrupt and keeps the connection up meanwhile. The async flash driver is there for when the
display can share an async bus.

## The reloader: getting onto NeoTime

A watch running InfiniTime or wasp-os has their bootloader at the start of flash. The reloader is
an ordinary app image for the old system, sent through its own update path, that replaces that
bootloader with NeoTime's. It carries three images: NeoTime's bootloader, the firmware, and the
recovery firmware (the same firmware, until there is a smaller one).

### What it does

The order comes from `neotime_boot::migration`, and its tests hold it to these rules:

1. **Wait for the charger.** It does nothing until power is present on P0.19, and shows why.
2. **Check its cargo.** Each image carries a CRC-32 from the build; a damaged download stops here.
3. **Stage the recovery firmware** in `RECOVERY_FIRMWARE` and **the firmware** in
   `FIRMWARE_UPDATE`, both in the external flash. The old system still starts during this; it has
   only lost its own files in the external flash.
4. **Write the bootloader** over the first pages of internal flash. From the first erase until the
   last word is in place, nothing on the watch starts. The bootloader is 8 KB, two pages: about a
   quarter of a second. Each copy is read back and compared, and retried up to three times.
5. **Mark the swap** in `BOOT_STATE`, so the bootloader's first start installs the staged
   firmware. Even if this step is lost, the bootloader is whole and the button hold installs the
   recovery firmware.
6. **Restart.**

The reloader never writes anywhere else in internal flash, so it keeps running from its own slot
while the first pages change under the old system. It copies its vector table into RAM first,
because the old vector table at address 0 is one of the pages it erases, and it feeds the old
bootloader's watchdog, which keeps running until the restart.

### Where it starts

The old system decides where an app's first two words are, so there are two builds:

| Feature | Old system | Slot start | Delivered as |
|---|---|---|---|
| `from-wasp-os` | wasp-os's bootloader on S112 | `0x19000` | a DFU zip with `--sd-req 0xB8`, so only that bootloader accepts it |
| `from-infinitime` | InfiniTime's MCUBoot | `0x8020` | an MCUBoot image (`tools/mcuboot_image.py`) inside a DFU zip, through InfiniTime's own DFU |

MCUBoot starts images 32 bytes into their slot, which is not an address the CPU accepts for a
vector table, so both builds put a two-word stub at the slot start and the table at the next
256-byte boundary.

The build takes the images from three environment variables, `NEOTIME_BOOTLOADER_BIN`,
`NEOTIME_FIRMWARE_BIN` and `NEOTIME_RECOVERY_BIN`, and refuses an image that is linked for the
wrong address or does not fit its slot. `tools/build_images.sh` builds everything into `dist/`.

### The risk, honestly

Without an SWD probe, a watch that cannot start cannot be brought back. The reloader keeps that
window to the bootloader write, about a quarter of a second, and only enters it on a charger. It
cannot make the window zero: the old and the new bootloader want the same first page.

Before it touches a watch, it runs on the PineTime emulator, with power cuts at many points,
through both starting systems; see "Proving it" below. Nothing here has run on a watch yet.

## Proving it on the emulator

The emulator (`PineTimeEmulator/`, Renode) models the chip's flash controller, the external flash
with real program and erase behaviour, the watchdog, the button and a battery pull. The test plan
for this work:

1. **Boot chain.** Flash the bootloader at 0 and the firmware at `FIRMWARE.start`. Run. The
   firmware's alive marker (`FIRMWARE_ALIVE_MARKER`) must hold `FIRMWARE_ALIVE_MAGIC`, and
   `BOOT_STATE` must read as booted after the firmware confirms itself.
2. **Update.** Place a second firmware in `FIRMWARE_UPDATE`, mark the swap, reset. The bootloader
   must swap it in and start it; without a confirmation the next reset must swap the old one back.
3. **Recovery.** Place an image in `RECOVERY_FIRMWARE`, erase `FIRMWARE`, hold the button through
   two resets. The recovery image must be running afterwards.
4. **Migration from wasp-os.** Start MBR, S112 and wasp-os's bootloader with the reloader in the
   app slot and the charger present. Afterwards the bootloader must be at 0, the staged images in
   the external flash, and after one more reset the firmware's alive marker set.
5. **Migration from InfiniTime.** The same from MCUBoot with the MCUBoot image.
6. **Power cuts.** Repeat 4 and 5 with a battery pull at points spread through the run. Each run
   must end in one of two states: the old system still starts, or NeoTime's bootloader is whole.
   The runs that end otherwise must all fall inside the bootloader write, and that window's
   length is reported.
7. **Watchdog.** With the button held, the firmware must stop feeding the watchdog and the watch
   must reset into the bootloader within about five seconds.
