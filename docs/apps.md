# Apps

NeoTime's first apps are ports of the apps in [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime)
and [wasp-os](https://github.com/wasp-os/wasp-os), written in the app language described in
[app-runtime.md](app-runtime.md) and [app-reference.md](app-reference.md). Where both firmwares have
the same app, the port merges them into one. Each app lives in `apps/<id>/app.tsx`, which names its
sources at the top. Watch faces are a separate package, listed in [faces.md](faces.md).

An app should be a thin layer over the shared parts in the firmware and the shared library: use
the SDK's widgets, components and functions rather than writing your own, and move anything a
second app needs into the library. RAM is the scarcest resource on the watch, and an app's own
code all takes RAM while it is open; see
[Building apps from shared parts](app-runtime.md#building-apps-from-shared-parts).

They compile to app images with `neotime-compiler`. They also type-check against `sdk/neotime`, a
stand-in for the `neotime` package, and every screen renders in a test.

The Screen column shows each app's main screen as the simulator draws it, and for the stopwatch,
the calculator and the launcher a second screen after some use. The pictures are the reference
screenshots in `harness/snapshots/screens/`, which the screenshot test keeps current; see
[Reference screenshots](testing.md#reference-screenshots).

## Checking the apps

From the repository root, with Node from `nix develop` or the system:

```sh
npm install
npm run check   # TypeScript over every app and face
npm test        # renders every screen of every app and face, plus their own tests
```

To compile them, inside `nix develop`:

```sh
for f in apps/*/app.tsx faces/*/face.tsx; do cargo run -q -p neotime-compiler -- --check "$f"; done
```

`-o <file>.ntapp` in place of `--check` writes the image, which the simulator opens with `--app`.

## Tools

| App | From | Notes | Screen |
|---|---|---|---|
| `stopwatch` | InfiniTime StopWatch, wasp-os stopwatch | Laps with split and total times. The side button starts and pauses. | <img src="../harness/snapshots/screens/app-stopwatch.png" width="120" alt="stopwatch"> <img src="../harness/snapshots/screens/app-stopwatch-running.png" width="120" alt="stopwatch running"> |
| `timer` | InfiniTime Timer, wasp-os timer | Pause, resume and reset. A progress ring, and a count of time since it rang. | <img src="../harness/snapshots/screens/app-timer.png" width="120" alt="timer"> |
| `alarm` | InfiniTime Alarm, wasp-os alarm | 12 alarms with weekday repeats, a 10 minute snooze, and a ring that stops itself after a minute. | <img src="../harness/snapshots/screens/app-alarm.png" width="120" alt="alarm"> |
| `pomodoro` | wasp-os pomodoro | A queue of up to 6 lengths, with presets and the buzz length set by swiping. | <img src="../harness/snapshots/screens/app-pomodoro.png" width="120" alt="pomodoro"> |
| `calculator` | InfiniTime Calculator, wasp-os calculator | Fixed point in millionths, up to 15 digits. wasp-os's brackets and powers are left out. | <img src="../harness/snapshots/screens/app-calculator.png" width="120" alt="calculator"> <img src="../harness/snapshots/screens/app-calculator-sum.png" width="120" alt="calculator sum"> |
| `dice` | InfiniTime Dice | 1 to 9 dice of 2 to 99 sides; shake to roll. | <img src="../harness/snapshots/screens/app-dice.png" width="120" alt="dice"> |
| `flashlight` | InfiniTime FlashLight, wasp-os flashlight | Three brightness levels and a red light. | <img src="../harness/snapshots/screens/app-flashlight.png" width="120" alt="flashlight"> |
| `battery` | InfiniTime BatteryInfo, PinetimeLite charging and low battery screens | The charge in a ring, charging, full, low or critical, and the voltage; a bar that fills while charging, and a warning to charge below 10 %. The watch opens it when the charger is plugged in or out, or the charge goes low. | <img src="../harness/snapshots/screens/app-battery.png" width="120" alt="battery"> |
| `phone-finder` | wasp-os phone_finder | Rings the phone. It cannot yet stop the ring from the watch. | <img src="../harness/snapshots/screens/app-phone-finder.png" width="120" alt="phone finder"> |
| `launcher` | NeoTime | The list of installed apps, each with its icon and name; a tap opens it. Any other launcher can replace it. | <img src="../harness/snapshots/screens/app-launcher.png" width="120" alt="launcher"> <img src="../harness/snapshots/screens/app-launcher-installed.png" width="120" alt="launcher installed"> |

## Health and phone

| App | From | Notes | Screen |
|---|---|---|---|
| `heart` | InfiniTime HeartRate, wasp-os heart | Measures on opening; a chart of the last 24 readings. | <img src="../harness/snapshots/screens/app-heart.png" width="120" alt="heart"> |
| `steps` | InfiniTime Steps, wasp-os step_counter and steplogger | Goal ring with yesterday's steps and a resettable trip meter, the last seven days, and any of them by hour, all from the system's step history. | <img src="../harness/snapshots/screens/app-steps.png" width="120" alt="steps"> |
| `sports` | wasp-os sports | A stopwatch with the steps taken since it started. | <img src="../harness/snapshots/screens/app-sports.png" width="120" alt="sports"> |
| `music` | InfiniTime Music, wasp-os music_player | Track, progress, play and skip by swipe, and a volume row. | <img src="../harness/snapshots/screens/app-music.png" width="120" alt="music"> |
| `notifications` | InfiniTime Notifications, wasp-os notification list | The latest 5 notifications from the phone and from apps, newest first; a tap reads one in full and Dismiss removes it. Opens by itself when a notification wakes the screen. | <img src="../harness/snapshots/screens/app-notifications.png" width="120" alt="notifications"> |
| `weather` | InfiniTime Weather, wasp-os weather | Conditions, low and high, and a 5 day forecast, all read from `current_weather`. | <img src="../harness/snapshots/screens/app-weather.png" width="120" alt="weather"> |
| `navigation` | InfiniTime Navigation | The next turn, its distance and the trip's progress. | <img src="../harness/snapshots/screens/app-navigation.png" width="120" alt="navigation"> |
| `metronome` | InfiniTime Metronome | Beats scheduled from the start time so they never drift; tap tempo. | <img src="../harness/snapshots/screens/app-metronome.png" width="120" alt="metronome"> |
| `motion` | InfiniTime Motion | Charts of the raw x, y and z acceleration. | <img src="../harness/snapshots/screens/app-motion.png" width="120" alt="motion"> |
| `level` | wasp-os level | A spirit level with a saved calibration. | <img src="../harness/snapshots/screens/app-level.png" width="120" alt="level"> |
| `beacon` | wasp-os beacon | Flashes the heart rate sensor's LED, while the app is in front. | <img src="../harness/snapshots/screens/app-beacon.png" width="120" alt="beacon"> |
| `disable-bluetooth` | wasp-os disa_b_l_e | Turns Bluetooth off and on again, without the reboot wasp-os needed. | <img src="../harness/snapshots/screens/app-disable-bluetooth.png" width="120" alt="disable bluetooth"> |

## Games and others

| App | From | Notes | Screen |
|---|---|---|---|
| `twos` | InfiniTime Twos, wasp-os play2048 | 2048 with a saved board and best score. | <img src="../harness/snapshots/screens/app-twos.png" width="120" alt="twos"> |
| `paddle` | InfiniTime Paddle | The paddle follows the finger. | <img src="../harness/snapshots/screens/app-paddle.png" width="120" alt="paddle"> |
| `puzzle15` | wasp-os puzzle15 | Every shuffle is solvable. | <img src="../harness/snapshots/screens/app-puzzle15.png" width="120" alt="puzzle15"> |
| `snake` | wasp-os snake | A 16 by 16 board. | <img src="../harness/snapshots/screens/app-snake.png" width="120" alt="snake"> |
| `four-in-a-row` | wasp-os four_in_a_row | 4 levels instead of 7, because the subset has no recursion. | <img src="../harness/snapshots/screens/app-four-in-a-row.png" width="120" alt="four in a row"> |
| `game-of-life` | wasp-os game_of_life | A 16 by 16 board that wraps at the edges, instead of 60 by 60. | <img src="../harness/snapshots/screens/app-game-of-life.png" width="120" alt="game of life"> |
| `infinipaint` | InfiniTime InfiniPaint | A 20 by 20 grid of cells instead of free drawing, since there is no canvas. | <img src="../harness/snapshots/screens/app-infinipaint.png" width="120" alt="infinipaint"> |
| `haiku` | wasp-os haiku | Built-in verses, since apps cannot read files. | <img src="../harness/snapshots/screens/app-haiku.png" width="120" alt="haiku"> |
| `morse` | wasp-os morse | Typing in Morse code with taps and swipes. | <img src="../harness/snapshots/screens/app-morse.png" width="120" alt="morse"> |
| `settings` | InfiniTime Settings, wasp-os settings | The watch-wide settings: brightness, screen timeout, always on, 12 or 24 hour clock, units, step goal, chimes, do not disturb and the theme. | <img src="../harness/snapshots/screens/app-settings.png" width="120" alt="settings"> |
| `hello`, `template` | wasp-os hello and template | Starting points for new apps. | <img src="../harness/snapshots/screens/app-hello.png" width="120" alt="hello"> <img src="../harness/snapshots/screens/app-template.png" width="120" alt="template"> |

## Not ported

| App | From | Why |
|---|---|---|
| `demo`, `gallery` | wasp-os | Need full-screen images and reading files. |
| `read_me`, `test` | wasp-os | Developer tools, not apps. |
| Quick settings, firmware update | both | Part of the system, not apps. |

## Limits

The ports use the API in [app-reference.md](app-reference.md) as `sdk/neotime` provides it: one
`CellGrid` for each game board, element writes, `range`, `count_where` and the text functions. They
fit the runtime's limits on state, handler writes, locals and loop steps. Four-in-a-row's search runs
a few steps a call on a 100 millisecond tick, so its hardest level thinks for up to about 3 seconds.

On the watch, `weather` shows no forecast until the phone's weather reaches it; the simulator shows
three days.
