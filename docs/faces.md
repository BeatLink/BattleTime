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

## Workarounds left

The ports were written before the runtime's API settled, and work around what it lacked then. Every
gap they found is now answered in [app-reference.md](app-reference.md) and provided by `sdk/neotime`,
so the faces can drop their workarounds: the `medium`, `giant` and display fonts, `BatteryIcon`,
`Scale` with `skip` and a needle, `AnalogClock` hand lengths and styles, `opacity`, `Image` with
`recolor` and the `pine_logo`, the night weather and `none` icons, `current_weather` instead of
each face's own copy and subscription, `is_heart_rate_running`, `heart_rate_status`,
`is_bluetooth_enabled`, `next_alarm`, `day_of_year`, `iso_week_of`, `to_upper` and `pad_start`.

The faces fit the runtime's limits. The analog face steps its hands by the minute with
`start_of_minute` while its second hand is off, so it redraws once a minute.
