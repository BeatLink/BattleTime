# Features

What NeoTime can do, added as each feature is finished. Work not done yet is in
[todo.md](todo.md); how the parts are built is in [architecture.md](architecture.md).

## On the watch

- **A watch face, a launcher and apps.** The firmware starts on the digital face. A launcher,
  itself an ordinary app that another could replace, lists the built-in apps with their icons: stopwatch, timer, flashlight, dice, calculator, snake, puzzle 15, steps and
  heart rate. Touches and the side button reach the open app.
- **The screen sleeps on its own.** It turns off after 15 seconds, unless the open app keeps it on.
- **Brightness.** The backlight has off, low, medium and high levels.
- **An always-on screen,** built with the `always-on-screen` feature. When the screen times out it
  dims and switches to the panel's low-power mode, and the face keeps drawing. It has not been tried
  on the watch yet.
- **Vibration** for apps that buzz.
- **Step counting.** The accelerometer counts today's steps for the steps app and faces.
- **Battery charge and charging state.**
- **The time from the phone,** set through the Bluetooth Current Time service.
- **The battery level on the phone,** through the standard Bluetooth Battery service, updated as it
  changes.
- **Themes from the companion app.** Six presets (dark, light, ocean, forest, sunset and contrast)
  or any colours, and the backlight level, set over Bluetooth and kept through a restart. The open
  app changes colour at once. See [companion-protocol.md](companion-protocol.md).
- **Apps remember where they were.** An app's saved state is written to the watch's storage when
  it closes or the screen turns off, and put back when it opens.
- **Text in clear, smooth fonts** made from JetBrains Mono and Open Sans.

## Apps and watch faces

- **30 apps** ported from InfiniTime and wasp-os, merged where both firmwares had the same app:
  stopwatch, timer, alarm, pomodoro, calculator, dice, flashlight, phone finder, heart rate, steps,
  sports, music, weather, navigation, metronome, motion, level, beacon, Bluetooth off, 2048, paddle,
  puzzle 15, snake, four in a row, Game of Life, paint, haiku, Morse, hello and a template. Each is
  described in [apps.md](apps.md).
- **18 watch faces** ported from InfiniTime, wasp-os and PinetimeLite, described in
  [faces.md](faces.md).
- **A 5 day weather forecast** in the weather app, read from the forecast the phone last sent.
- **Screens with room to breathe.** Content keeps 8 pixels from the screen edges and controls keep
  apart, except game boards and dials that fill the screen.

## Updates and recovery

- **Firmware updates over Bluetooth** with Nordic's legacy DFU, the protocol InfiniTime uses, so
  Gadgetbridge, ITD, Siglo and NeoTime's own tools can update the watch unchanged. See
  [updates.md](updates.md). The image's CRC is checked by reading it back from the flash, and the
  status page shows the kilobytes received.
- **A bad update rolls itself back.** A new firmware runs on trial until it confirms itself; if it
  never does, the bootloader puts the previous firmware back.
- **A recovery firmware** kept in external flash, installed by holding the side button at start.
  A firmware update can also replace the recovery firmware.
- **The side button stops a stuck watch.** The firmware feeds the watchdog only while the button
  is up, so holding it restarts the watch.
- **Status on screen.** The status page shows the firmware's mode in plain words and a colour:
  black when confirmed, amber on trial, red after a rollback and purple for the recovery firmware.
  The bootloader turns the screen blue while it waits for a recovery hold or installs an update.
- **Bootloader updates over Bluetooth,** with the bootloader updater: sent like any firmware
  update, it writes the new bootloader only on the charger and hands the watch back to the firmware
  that was running. It has run only in the emulator.
- **Moving a watch from InfiniTime or wasp-os onto NeoTime** through that system's own update
  path, with the reloader. It moved a watch from wasp-os on 2026-10-11; the InfiniTime path has
  run only in the emulator.

## For app makers

- **Apps written in Preact.** Apps and faces are TSX with Preact signals, compiled ahead of time to
  small app images by `neotime-compiler`. The language is in [app-reference.md](app-reference.md)
  and [compiler.md](compiler.md).
- **A stand-in `neotime` package** in `sdk/neotime`, so apps type-check and their screens render in
  tests on the computer, with `npm run check` and `npm test`.
- **A simulator** that runs the drivers and app images in a desktop window, and saves screenshots.
  See [simulator.md](simulator.md).
