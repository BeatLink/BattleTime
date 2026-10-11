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
