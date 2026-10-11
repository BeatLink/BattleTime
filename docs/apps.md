# Apps

NeoTime's first apps are ports of the apps in [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime)
and [wasp-os](https://github.com/wasp-os/wasp-os), written in the app language described in
[app-runtime.md](app-runtime.md) and [app-reference.md](app-reference.md). Where both firmwares have
the same app, the port merges them into one. Each app lives in `apps/<id>/app.tsx`, which names its
sources at the top. Watch faces are a separate package, listed in [faces.md](faces.md).

None of them runs on the watch yet. They type-check against `sdk/neotime`, a stand-in for the
`neotime` package, and every screen renders in a test. They will be compiled once the compiler
exists.

## Checking the apps

From the repository root, with Node from `nix develop` or the system:

```sh
npm install
npm run check   # TypeScript over every app and face
npm test        # renders every screen of every app and face, plus their own tests
```

## Tools

| App | From | Notes |
|---|---|---|
| `stopwatch` | InfiniTime StopWatch, wasp-os stopwatch | Laps with split and total times. The side button starts and pauses. |
| `timer` | InfiniTime Timer, wasp-os timer | Pause, resume and reset. A progress ring, and a count of time since it rang. |
| `alarm` | InfiniTime Alarm, wasp-os alarm | 12 alarms with weekday repeats, a 10 minute snooze, and a ring that stops itself after a minute. |
| `pomodoro` | wasp-os pomodoro | A queue of up to 6 lengths, with presets and the buzz length set by swiping. |
| `calculator` | InfiniTime Calculator, wasp-os calculator | Fixed point with three decimals, up to 99999.999. wasp-os's brackets and powers are left out. |
| `dice` | InfiniTime Dice | 1 to 9 dice of 2 to 99 sides; shake to roll. |
| `flashlight` | InfiniTime FlashLight, wasp-os flashlight | Three brightness levels and a red light. |
| `phone-finder` | wasp-os phone_finder | Rings the phone. It cannot yet stop the ring from the watch. |

## Health and phone

| App | From | Notes |
|---|---|---|
| `heart` | InfiniTime HeartRate, wasp-os heart | Measures on opening; a chart of the last 24 readings. |
| `steps` | InfiniTime Steps, wasp-os step_counter and steplogger | Goal ring, trip meter, and a history screen of the week and today by hour, which the app logs itself. |
| `sports` | wasp-os sports | A stopwatch with the steps taken since it started. |
| `music` | InfiniTime Music, wasp-os music_player | Track, progress, play and skip by swipe, and a volume row. |
| `weather` | InfiniTime Weather, wasp-os weather | Conditions, low and high, and a 5 day forecast. |
| `navigation` | InfiniTime Navigation | The next turn, its distance and the trip's progress. |
| `metronome` | InfiniTime Metronome | Beats scheduled from the start time so they never drift; tap tempo. |
| `motion` | InfiniTime Motion | Charts of the raw x, y and z acceleration. |
| `level` | wasp-os level | A spirit level with a saved calibration. |

## Games and others

| App | From | Notes |
|---|---|---|
| `twos` | InfiniTime Twos, wasp-os play2048 | 2048 with a saved board and best score. |
| `paddle` | InfiniTime Paddle | The paddle follows the finger. |
| `puzzle15` | wasp-os puzzle15 | Every shuffle is solvable. |
| `snake` | wasp-os snake | A 16 by 16 board. |
| `four-in-a-row` | wasp-os four_in_a_row | 4 levels instead of 7, because the subset has no recursion. |
| `game-of-life` | wasp-os game_of_life | A 16 by 16 board that wraps at the edges, instead of 60 by 60. |
| `infinipaint` | InfiniTime InfiniPaint | A 20 by 20 grid of cells instead of free drawing, since there is no canvas. |
| `haiku` | wasp-os haiku | Built-in verses, since apps cannot read files. |
| `morse` | wasp-os morse | Typing in Morse code with taps and swipes. |
| `hello`, `template` | wasp-os hello and template | Starting points for new apps. |

## Not ported

| App | From | Why |
|---|---|---|
| `beacon` | wasp-os | Needs control of the heart rate sensor's LED. |
| `disa_b_l_e` | wasp-os | Needs to turn Bluetooth off and on. |
| `demo`, `gallery` | wasp-os | Need full-screen images and reading files. |
| `read_me`, `test` | wasp-os | Developer tools, not apps. |
| Launcher, settings, notifications, quick settings, firmware update | both | Part of the system, not apps. |

## What the apps still need

The ports were written against the API as it stands, and work around what it lacks. These are the
gaps they found, roughly in order of how many apps they affect:

- **A board of cells as one widget,** such as `CellGrid({ columns, rows, cells, palette })`. The
  games now make one widget per cell, up to 400, which will not fit the UI heap.
- **Fonts:** a size between normal and large.
- **Array and text helpers:** writing one element of an array, a compile-time `range(count)`,
  loops or `includes` and `index_of` in a `computed`, and text length and slices.
- **Widgets:** a `Chart` with several series, and an `Arc` that can be dragged.
- **Effects:** stopping `find_phone`, asking the phone to resend the music state, the heart rate
  sensor's LED, and turning Bluetooth off.
- **Subscriptions only while open,** for `motion`, so a shake in the background does not roll the
  dice.
- **Rules to confirm:** whether a screen's `onSwipe` replaces the system swipes, whether an `at`
  time already past fires at once, whether early `return` is allowed in app functions, which
  bitwise operators are allowed, and how large the step limit is (four-in-a-row's search may need
  about 100,000 steps a move).
