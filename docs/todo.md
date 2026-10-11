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
