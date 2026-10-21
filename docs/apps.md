# All apps

Every app BattleTime ships, by the name the launcher shows, grouped as the launcher groups them at first. Apps install and uninstall
from the companion app's store, the companion can move an app to another group, and
[Software](apps/software.md) turns each one on or off. Watch faces have their own page,
[Watch faces](faces.md).

Most apps come from [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime) and
[wasp-os](https://github.com/wasp-os/wasp-os), merged into one where both had the same app; each
app's page says where it came from.

## Built into the firmware

These screens are part of the firmware, so they are always there and never listed in the launcher.

| Screen | What it does |
|---|---|
| [Notifications](apps/notifications.md) | The latest notifications, newest first, each with an icon coloured by its kind; a swipe down on the face opens it, and a preview pops up as one arrives. |
| [Quick settings](apps/quick-settings.md) | A pull-down shade below the notifications: brightness, flashlight, do not disturb, Bluetooth, always on and Settings. |

## Time

| App | What it does |
|---|---|
| [Alarm](apps/alarm.md) | Up to twelve alarms, once or on chosen weekdays, with snooze. |
| [Timer](apps/timer.md) | A countdown of up to 99 minutes that keeps running while the app is closed. |
| [Stopwatch](apps/stopwatch.md) | A stopwatch with laps, still counting while the watch sleeps. |
| [Pomodoro](apps/pomodoro.md) | Work and break sessions in turn, with a long break every few rounds. |

## Phone

These need the phone or computer connected through the companion app.

| App | What it does |
|---|---|
| [Call](apps/call.md) | Opens by itself when a call comes in, to accept, reject or mute it and end it. |
| [Music](apps/music.md) | Plays, pauses and skips the phone's music, and changes its volume. |
| [Navigation](apps/navigation.md) | Turn-by-turn directions from the phone. |
| [Weather](apps/weather.md) | The current weather and the coming days' forecast from the phone. |
| [Finder](apps/phone-finder.md) | Makes the phone ring so you can find it. |
| [Remote](apps/remote.md) | Up to nine buttons that each run an action on the phone or computer. |
| [Notes](apps/notes.md) | A read-only text note sent from the companion, such as a shopping list. |
| [QR code](apps/qr-code.md) | Up to eight QR codes made in the companion, shown for another phone to scan. |
| [Prompt](apps/prompt.md) | A yes or no question from the companion, answered on the watch. |
| [SOS](apps/sos.md) | Hold the red button for five seconds to send the phone an SOS, while the light, screen and motor flash SOS. |

## Health

| App | What it does |
|---|---|
| [Heart](apps/heart.md) | Measures the heart rate and charts the latest readings. |
| [Steps](apps/steps.md) | Today's steps against the goal, the last seven days and a day by hour. |
| [Sports](apps/sports.md) | A stopwatch that counts steps too, keeping the last five sessions for the companion. |

## Games

| App | What it does |
|---|---|
| [Dice](apps/dice.md) | Roll one to nine dice of up to 99 sides, by tap or shake. |
| [4 in a row](apps/four-in-a-row.md) | Four in a Row against the watch, at four levels. |
| [Life](apps/game-of-life.md) | Conway's Game of Life on a 16 by 16 board that wraps at its edges. |
| [Paint](apps/infinipaint.md) | Finger painting on a 20 by 20 grid in eight colours. |
| [Paddle](apps/paddle.md) | Keep the ball in play with a paddle that follows your finger. |
| [15](apps/puzzle15.md) | The 15 sliding-tile puzzle, with a move count and best result. |
| [Snake](apps/snake.md) | Classic Snake on a 16 by 16 board, steered by swipes. |
| [2048](apps/twos.md) | 2048: slide tiles to merge equal numbers. |

## Tools

| App | What it does |
|---|---|
| [Calculator](apps/calculator.md) | Adds, subtracts, multiplies and divides numbers of up to fifteen digits. |
| [Flashlight](apps/flashlight.md) | Turns the screen into a white or red light, at three brightness levels. |
| [Beacon](apps/beacon.md) | Flashes the heart rate sensor's green LED at a chosen brightness and rate. |
| [Level](apps/level.md) | A spirit level whose bubble turns green when the watch is level. |
| [Motion](apps/motion.md) | Charts the accelerometer's readings on each axis. |
| [Metronome](apps/metronome.md) | Beats on the vibration motor, with tap tempo. |
| [Morse](apps/morse.md) | A notepad you type in Morse code with swipes. |
| [Survival](apps/survival.md) | Emergency guides by topic, such as CPR, being lost in the wild or protesting safely, chosen in the companion and read offline. |
| [Haiku](apps/haiku.md) | A new short verse each time it opens. |
| [Hello](apps/hello.md) | The smallest app, a starting point for reading app code. |
| [Template](apps/template.md) | A starting point for a new app that answers every kind of input. |

## System

| App | What it does |
|---|---|
| [Apps](apps/launcher.md) | The launcher: lists the installed apps by group and opens the one you tap. |
| [Settings](apps/settings.md) | The watch's settings: screen, waking, clock, units, step goal, face and theme. |
| [Software](apps/software.md) | Turns each installed app and face on or off. |
| [About](apps/about.md) | Firmware, battery, memory and hardware details, a page each. |
| [Files](apps/files.md) | Browses the watch's storage a folder at a time, with sizes. |

An app whose group the watch does not have goes to Other.

## Not ported

| App | From | Why |
|---|---|---|
| `demo`, `gallery` | wasp-os | They need full-screen pictures. |
| `disa_b_l_e` | wasp-os | The Bluetooth switch in Settings and quick settings does the same. |
| `read_me`, `test` | wasp-os | Developer tools, not apps. |
| Firmware update | both | Part of the system, not an app; see [Firmware updates](updates.md). |

To write an app of your own, start with [writing an app](writing-apps.md).
