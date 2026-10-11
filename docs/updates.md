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
- advertises as "NeoTime" with the DFU service, and confirms itself to the bootloader once a phone
  has exchanged data with it, so a firmware whose radio does not really work rolls back by itself;
  advertising alone proves nothing, since the host is told it started whether or not anything
  went out on air;
- confirms itself at start instead when a rollback could not help: when the replaced firmware,
  which embassy-boot keeps one page into the update slot (`PREVIOUS_FIRMWARE`), is not a firmware
  at all, as after a migration, or when the running image is the recovery image, which must stay;
- serves the DFU service on every connection, shows progress on the screen, and resets when the
  protocol asks;
- mounts the file system, so that an update which breaks storage is noticed on the screen;
- writes an alive marker into the last words of RAM, which an emulator checks.

### The status page

At start, and again while an update arrives, the firmware shows a status page. Its background
colour and second and third lines say which mode the firmware is in (`firmware/src/status.rs`,
colours in `neotime_boot::status_colour`):

| Mode | Background | Lines | How the firmware knows |
|---|---|---|---|
| Confirmed | black | Firmware OK | `BOOT_STATE` says booted |
| On trial | dark amber | New firmware on trial / Connect to keep it | `BOOT_STATE` says swap; after the first Bluetooth exchange it turns black with "Update kept" |
| Rolled back | dark red | Update failed / Previous one restored | `BOOT_STATE` says revert, which stays until the next update |
| Recovery | purple | Recovery firmware / Send a new firmware | the bootloader's note, or a build with the `recovery` feature |

The other lines are the version, the file system ("Files OK", "Files set up", "Files damaged"
or "No flash chip found"), Bluetooth ("Ready for updates" or "Phone connected") and the size received
during an update. The bootloader's own blue screen is described in
[bootloader.md](bootloader.md#the-status-screen).

A recovery firmware installed by the button hold shows as recovery only until its next restart,
because the note is not kept. A firmware built with `--features recovery` always does, and it is
the image meant for `RECOVERY_FIRMWARE`.

A panic halts rather than resets, so the watchdog brings the watch back to the bootloader instead of
the firmware restarting in a loop. It is 105 KB of flash and 25 KB of static RAM, 16 KB of that heap. Its screen writes and flash
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

The larger risk is after it. Once the reloader has run, the only Bluetooth on the watch is
NeoTime's own controller, and a controller that does not work on real hardware leaves a watch
that starts but can never take another update. The emulator cannot rule that out, because it
does not model the radio's timing. So the first step on a watch is the rehearsal below, which can
always be undone.

A panic in the reloader halts it rather than resetting: a reset would restart it under the old
bootloader, which would feed the watchdog again, and the watch would loop with no way into the old
update mode. Halting lets the watchdog reset the watch, which the old bootloader takes as a request
for its update mode.

When the reloader stops on purpose, for a damaged download, a missing flash chip or a write that
fails three times, it says so on the screen and keeps feeding the watchdog only while the button is
up, as it does while it waits for the charger. Holding the button for a few seconds then lets the
watchdog reset the watch: into the old update mode before the bootloader write, or into NeoTime's
bootloader after it, where holding on installs the recovery firmware.

### The rehearsal: proving Bluetooth first, under wasp-os

`firmware` built with `--features wasp-os-rehearsal` is an app for wasp-os, linked at `0x19000`
and sent through wasp-os's own update path. It runs NeoTime's whole Bluetooth path, the controller,
TrouBLE and the DFU service, while wasp-os's bootloader and SoftDevice stay where they are. It writes
to neither flash chip: NeoTime's boot state page sits inside wasp-os's SoftDevice, and the external
flash holds wasp-os's files. An upload lands in a 16 KB RAM buffer and is only checked against its
CRC. Its watchdog takeover matches wasp-os's settings, which keep counting under a debugger.

On the watch, the rehearsal must show that the watch advertises as "NeoTime", that a phone or the
computer connects, and that a 16 KB upload (`dist/rehearsal-upload.zip`) arrives with its CRC
matching. When an upload finishes, or the phone sends the reset command, the rehearsal writes 0xA8
to `GPREGRET`, which wasp-os's bootloader reads as a request for its update mode, and restarts; so
one rehearsal can follow another without the button.

The first runs on the watch found three things the emulator could not:

- **The 32 kHz clock.** The rehearsal stopped the low-frequency clock and waited for it to stop,
  but while the watchdog runs the chip keeps it going, so the wait never ended and the watchdog
  sent the watch back to wasp-os. The rehearsal now leaves the clock alone. It also records each
  start-up stage, and any panic or fault, in words of wasp-os's bootloader's retained RAM at
  `0x200039CC`, and shows the previous run's record on screen.
- **The controller works.** NeoTime's own controller advertised, accepted connections, answered GATT
  reads and carried a whole 16 KB upload, every progress receipt matching and the CRC verified on
  the watch, in 48 seconds.
- **Discovery cut a UUID short.** TrouBLE sent the control point's 128-bit UUID cut to 16 bits, so
  the update tools could not find it. `third_party/trouble-host` carries the fix (see its
  `NEOTIME.md`), and a controller test reproduces the watch's exact layout. With the fix, wasp-os's
  own `dfu.py` carried the same upload in 61 seconds.

Holding the button for five seconds, or any fault, ends in a watchdog reset, which wasp-os's
bootloader takes as a request for its update mode, so wasp-os can be sent back at any time. Only
after the rehearsal passes does the reloader go to the watch.

### The storage rehearsal: the external flash and a real update

`firmware` built with `--features wasp-os-storage-rehearsal` is the same app with one difference:
it uses the external flash, which erases wasp-os's files there. At start-up it writes, reads back
and erases 32 KB of the update slot, then formats a littlefs file system, writes a file, remounts
and reads it back, and shows "flash, files ok" when all of it passes. An upload then goes through
the real `UpdateReceiver` into the real update slot, with the boot state kept in RAM, since
NeoTime's boot state page still lies inside wasp-os's SoftDevice. The internal flash is never
written.

On the watch it showed "flash, files ok", and the real firmware, 107 KB, went into the update
slot through wasp-os's own `dfu.py` in 6 minutes 57 seconds, its CRC checked by reading the image
back from the chip.

### On the watch

On 2026-10-11 the reloader went to a PineTime running wasp-os on S112, through wasp-os's own
update mode (238 KB in 4 minutes 7 seconds), and the watch came up running NeoTime seconds later,
with its file system found and the firmware confirmed. A firmware update sent to NeoTime itself then
took 1 minute 51 seconds to upload. The bootloader swapped it in, it started, and it confirmed
itself after its first Bluetooth exchange.

Recovery works on the watch as well. Holding the button reset the watch into the bootloader, which
buzzed after five more seconds. It swapped in the recovery firmware, which started, and that
firmware confirmed itself after its first Bluetooth exchange.

## Proving it on the emulator

The emulator (`PineTimeEmulator/`, Renode) models the chip's flash controller, the external flash
with real program and erase behaviour, the watchdog, the button and a battery pull. The suites are
in `tests/emulator/`, one Robot file per area, run from the emulator's shell, each in its own
process:

```sh
renode-test --variable EMULATOR:<emulator checkout> --exclude long tests/emulator/<suite>.robot
```

The `long` tag marks the power-cut sweeps, which take hours of wall-clock time. The emulator has
already caught two faults that would have stranded a watch: the reloader sent 256-byte pages from
its own flash, which embassy's SPI driver cannot do (it panics above 255 bytes), and the firmware
read the replaced image from the wrong page after a swap. The plan:

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
8. **Rehearsal.** Under wasp-os's bootloader the rehearsal must start, no write to the flash
   controller may come from its code, both flashes must be unchanged while it runs, and the
   button hold must end in wasp-os's update mode rather than another start of the rehearsal.
9. **Storage rehearsal.** The same, except the external flash may change; no write to the flash
   controller may come from its code, so the internal flash stays as wasp-os left it.
