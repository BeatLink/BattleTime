# To do

Work that is known to be needed but not done yet.

## Hardware tests

- [ ] **Touch panel on the PineTime.** `neotime-touch` has only run against fake hardware. Once the
  board layer can start a driver, check on the watch that:
  - `start` succeeds and `read_chip_info` gives a sensible chip ID (0xb4 or a near value);
  - taps, long presses and slides in all four directions come through as the right `Gesture`;
  - a drag streams reports while the finger stays down, and the finger lifting is reported;
  - positions match the display, with column 0 and row 0 at the top left;
  - `sleep` stops the interrupts, and `sleep_until_double_tap` wakes on a double tap only;
  - `wake` brings back the settings `start` was given.

- [ ] **Accelerometer on the PineTime.** `neotime-accelerometer` has only run against fake
  hardware. On the watch, check that:
  - `start` succeeds and names the chip the watch really has, which on this watch is a BMA425;
  - lying flat with the screen up reads about +1000 milli-g on z, and tilting it reads the
    expected signs on x and y, which settles whether `Orientation::PINETIME` is right;
  - walking raises `read_step_count`, and `reset_step_count` sets it back to 0;
  - the upload still works with chunks larger than 16 bytes, which would make starting faster.

- [ ] **Bluetooth controller on the PineTime.** `neotime-bluetooth` has only run against a fake
  radio. Once there is firmware to put it in, check on the watch, with a phone and with `btmon` on
  the computer, that:
  - advertisements show up on all three channels, with the right name and address;
  - a scan request gets the scan response;
  - a phone connects, and the connection survives the phone changing its interval;
  - GATT reads and notifications work, and Gadgetbridge can connect;
  - the link drops cleanly when the phone leaves or goes out of range;
  - which nRF52832 revision the watch has (FICR at `0x10000134`), since the radio errata
    workaround depends on it.

## Bluetooth controller

See [bluetooth.md](bluetooth.md) for the details of each.

- [ ] **Encryption and pairing.** Answer `LL_ENC_REQ` and encrypt packets with the chip's CCM and
  ECB peripherals, so phones that insist on pairing can connect.
- [ ] **Sleep between connection events.** Wake from an RTC compare shortly before each event and
  start the 64 MHz crystal then, instead of keeping the crystal and TIMER0 running. This is the
  controller's biggest battery cost.
- [ ] **Peripheral latency.** Skip the connection events the central allows the watch to skip.
- [ ] **Faster transfers.** Data length extension, the 2 Mbit PHY and channel selection algorithm
  #2, mainly to speed up firmware updates.
- [ ] **Connection parameter request** from the peripheral. TrouBLE falls back to L2CAP meanwhile.
- [ ] **Skip the HCI byte layer** if flash gets tight. The controller could take TrouBLE's
  commands directly instead of encoding and decoding HCI bytes, which would save a few KB.

## Firmware updates

- [ ] **Update protocol over Bluetooth.** Not decided yet. The candidate is Nordic's legacy DFU
  service, as InfiniTime uses, so Gadgetbridge, ITD and Siglo can update NeoTime without changes.
  It would sit on the `services/updates` crate from the bootloader work.
- [ ] **Reloader image.** A firmware image, sent as a wasp-os or InfiniTime update, that moves the
  watch to NeoTime's bootloader. Stage NeoTime and the recovery firmware in external flash first,
  keep the window where a power cut bricks the watch as short as possible, refuse to run unless
  the watch is charging, and prove it in the Renode emulator (`PineTimeEmulator/`), power cuts
  included, before it touches a watch. Without SWD, a mistake here cannot be undone.
- [ ] **Feed the watchdog only while the button is up,** from the first firmware on. It is the only
  way back to the bootloader without SWD.
