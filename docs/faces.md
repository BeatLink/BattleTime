# Watch faces

NeoTime's first watch faces are ports of every face in
[InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime) and [wasp-os](https://github.com/wasp-os/wasp-os),
written in the app language described in [app-runtime.md](app-runtime.md) and
[app-reference.md](app-reference.md). They are a package of their own, `faces/`, beside the apps in
[apps.md](apps.md). Each face lives in `faces/<id>/face.tsx`, which names its sources at the top,
and declares `kind: "face"`.

None of them runs on the watch yet; see [apps.md](apps.md#checking-the-apps) for how they are
checked.

| Face | From | Notes |
|---|---|---|
| `digital` | InfiniTime WatchFaceDigital, wasp-os clock | Status icons, weather, heart rate and steps. |
| `analog` | InfiniTime WatchFaceAnalog, wasp-os chrono | A long press turns the second hand off to save power. |
| `terminal` | InfiniTime WatchFaceTerminal | |
| `pinetime-style` | InfiniTime WatchFacePineTimeStyle | A long press opens its colour and layout settings. |
| `casio` | InfiniTime WatchFaceCasioStyleG7710 | Week number and day of the year. |
| `infineat` | InfiniTime WatchFaceInfineat | Palettes and settings on a long press. |
| `pride-flag` | InfiniTime WatchFacePrideFlag | A long press chooses the flag. |
| `week-clock` | wasp-os week_clock | |
| `dual-clock`, `fibonacci-clock`, `word-clock`, `resistor-clock` | wasp-os faces of the same names | |

InfiniTime's own fonts, such as the 7-segment and dot matrix ones, are replaced by the four standard
sizes until the shared library has display fonts.

A face that has settings, such as PineTimeStyle, Infineat and the pride flag, opens them on a long
press as a second screen.

## What the faces still need

- **Fonts:** a size of about 30 pixels, one of 110 to 150 pixels for big clocks, and display
  fonts such as 7-segment, dot matrix and condensed.
- **System values:** whether the heart rate sensor is running, whether Bluetooth is on, the next
  alarm, and the weather the system last received with when it arrived, so faces need not each
  subscribe and keep a copy. Weather is not covered by any permission yet; the faces use
  `background`.
- **Widgets:** `RichText` for several colours on one line, a battery icon with its percent, a
  needle gauge, hand lengths and styles on `AnalogClock`, skipping ticks on `Scale`, and
  `opacity` on `Line`.
- **Images:** the pine logo in the shared library, a recolour prop on `Image`, and icons for alarm,
  night weather and "none".
- **Functions:** `day_of_year`, `iso_week_of`, padding with spaces, and upper-case weekday and month
  names.
- **Rules to confirm:** whether a face may have a settings screen, as app-runtime.md gives a face
  one screen.
