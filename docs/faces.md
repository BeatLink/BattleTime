# Watch faces

NeoTime's first watch faces are ports of every face in
[InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime), [wasp-os](https://github.com/wasp-os/wasp-os)
and [PinetimeLite](https://github.com/joaquimorg/PinetimeLite) (its `developLite` branch, version 0.7.0),
written in the app language described in [app-runtime.md](app-runtime.md) and
[app-reference.md](app-reference.md). They are a package of their own, `faces/`, beside the apps in
[apps.md](apps.md). Each face lives in `faces/<id>/face.tsx`, which names its sources at the top,
and declares `kind: "face"`.

They compile with the apps; see [apps.md](apps.md#checking-the-apps) for how they are checked.

Faces follow the same structure as apps: status icons, step and heart rate badges, weather and the
clock's parts come from the shared parts rather than being drawn again in each face; see
[Building apps from shared parts](app-runtime.md#building-apps-from-shared-parts).

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
| `lite-digital` | PinetimeLite WatchFaceDigital | The time on a white band, with a bar that grows with the seconds. |
| `lite-analog` | PinetimeLite WatchFaceAnalog | Both of its dials, the classic one and the rays one with steps; a long press switches. |
| `lite-rings` | PinetimeLite WatchFaceMinimal | Rings for the hour, minute and second around the time. |
| `lite-pong` | PinetimeLite WatchFacePong | Two rackets play an endless rally under the time. |
| `lite-photo` | PinetimeLite WatchFaceCustom | The time over a backdrop with a blinking colon; a long press picks the backdrop. |
| `lite-weather` | PinetimeLite WatchFaceWeather | Big stacked digits beside the weather icon. |

A face that has settings, such as PineTimeStyle, Infineat and the pride flag, opens them on a long
press as a second screen.

## Limits and what is left

The faces use the API in [app-reference.md](app-reference.md) as `sdk/neotime` provides it: the
`giant` and `dot_matrix` fonts, `BatteryIcon` (in one colour where the original
keeps one), `Scale` with angles, skipped ticks and a needle, `AnalogClock` hand lengths, `opacity`,
the `pine_logo`, `current_weather` with sunrise and sunset for night icons, and the heart rate,
Bluetooth and alarm system values. They fit the runtime's limits, and only faces that
show seconds redraw every second. The analog face steps its hands by the minute while its second
hand is off.

The PinetimeLite faces have workarounds of their own. `lite-digital` and `lite-weather` use the
`condensed` display font for their smaller numbers, and `lite-weather` draws its big digits plain in
`huge` rather than striped. `lite-analog` uses plain hands for PinetimeLite's thin stem and thick body,
and `lite-photo` uses plain colours for PinetimeLite's three Pixabay photos; the user's own photo,
sent from the phone, would be better than shipping those.

Still open: each display font comes in one size (`dot_matrix` about 40 pixels and `condensed`
about 48), and Casio draws its time and date in the standard font.
