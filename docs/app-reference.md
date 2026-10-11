# App reference

This page lists what a NeoTime app can read, which events it can answer, which widgets it can show
and which effects it can ask for. [app-runtime.md](app-runtime.md) explains how apps are written
and run. Everything here is **planned**; names and fields may change as the runtime is built.

Every name comes from the `neotime` package. Widgets are imported from `neotime/widgets` and ready-made
components from `neotime/components`.

## System values

System values are read-only signals the runtime keeps current. Reading one in a view or a
`computed` makes that binding depend on it, so it redraws when the value changes and at no other
time.

| Value | Type | Holds |
|---|---|---|
| `now` | `Instant` | The current time. A binding that reads it redraws as often as its format shows, such as once a minute for `HH:MM`. |
| `utc_offset` | `Duration` | The local time zone's offset, set by the phone. |
| `battery_percent` | `Int` | The battery charge, 0 to 100. |
| `is_charging` | `Bool` | Whether the watch is on its charger. |
| `steps_today` | `Int` | Steps since local midnight. |
| `heart_rate` | `Int` | The last heart rate measured, in beats per minute, or 0 if none. |
| `heart_rate_measured_at` | `Instant` | When `heart_rate` was measured. |
| `is_phone_connected` | `Bool` | Whether the phone is connected over Bluetooth. |
| `unread_notifications` | `Int` | How many notifications are unread. |
| `is_do_not_disturb` | `Bool` | Whether do not disturb is on. |
| `brightness` | `Int` | The backlight level, 1 to 3. |
| `settings` | object | The user's choices apps should follow: `uses_24_hour_clock`, `uses_metric_units`, `step_goal`, `theme` colours. |
| `current_screen` | `Text` | The name of the app's screen on display. |

## Events

Events are what call an app's handlers. Each handler receives the event's fields as its argument.
Handlers run one at a time, to the end, and never overlap.

### Touch

Touch events belong to the widget that was touched. A widget with no handler for an event lets it
pass to the widget behind it, and then to the screen.

| Prop | Fields | When |
|---|---|---|
| `onClick` | `x`, `y` | A short tap. |
| `onLongPress` | `x`, `y` | A finger held still for about half a second. |
| `onDoubleClick` | `x`, `y` | Two taps close together. The touch panel reports this itself. |
| `onPress`, `onRelease` | `x`, `y` | A finger landing and lifting, for widgets that show a pressed state. |
| `onDrag` | `x`, `y`, `start_x`, `start_y` | A finger moving while down, for sliders and drawing. At most once per redraw. |
| `onSwipe` | `direction` | A slide `up`, `down`, `left` or `right`, set on a screen. |

Swipes the screen does not take keep their system meaning. By default, right goes back and down
opens the quick settings.

### Side button

| Prop | Fields | When |
|---|---|---|
| `onButton` | `kind`: `press` or `long_press` | The side button, set on a screen. |

Without a handler the button goes back, and from the watch face it turns the screen off. Watch faces
cannot take the button. Holding it for 5 seconds always resets the watch, whatever the app does.

### Widget values

| Prop | Widgets | Fields |
|---|---|---|
| `onChange` | `Slider`, `Switch`, `Checkbox`, `Radio`, `Roller`, `Spinbox`, `Dropdown`, `TextArea` | `value` |
| `onPageChange` | `Pages`, `Tabs` | `page` |
| `onScrollEnd` | `Scroll`, `List` | `offset` |

A widget whose value is bound to a signal, such as `<Slider value={volume} />` without `.value`,
writes the new value back itself, so `onChange` is only needed for anything else to happen.

### App and screen life

| Prop | Set on | When |
|---|---|---|
| `onOpen` | `define_app` | The app comes to the front. |
| `onClose` | `define_app` | The app leaves the front, for another app or the watch face. |
| `onShow`, `onHide` | a screen | A screen comes into view or leaves it. |
| `onScreenOff`, `onScreenOn` | `define_app` | The display turns off or on while the app is in front. |

Most apps need none of these, because state persists and bindings redraw themselves.

### Subscriptions

Subscriptions are declared in `define_app({ subscriptions: [...] })`. They call their handler even
while the app is closed; see [Background work](app-runtime.md#background-work).

| Subscription | Fields | When |
|---|---|---|
| `every(period, handler)` | `time` | Repeatedly. Periods shorter than a minute run only while the app is open. |
| `at(instant, handler)` | `time` | Once, at an absolute time. The instant may be a signal, so changing it reschedules. |
| `daily(time_of_day, handler)` | `time` | Every day at a local time, such as an alarm. |
| `clock_change(unit, handler)` | `time` | The local `minute`, `hour` or `day` changes, such as a reset at midnight. Also fires when the phone sets the clock. |
| `heart_rate(period, handler)` | `beats_per_minute`, `confidence` | A measurement every `period`. Several apps asking share one measurement. |
| `steps(handler)` | `steps_today` | The step count changes, at most once a minute. |
| `step_goal(handler)` | `steps_today` | The day's step goal is reached. |
| `motion(kind, handler)` | `kind` | The accelerometer sees `wrist_raise`, `shake`, `tap`, `double_tap`, `still` or `moving`. Which ones exist depends on the chip; the BMA425 has them all. |
| `battery(kind, handler)` | `percent` | The battery goes `low`, or charging `starts`, `stops` or `completes`. |
| `phone(kind, handler)` | `is_connected` | The phone `connects` or `disconnects`. |
| `notification(handler)` | `id`, `app`, `title`, `body`, `time` | A notification arrives from the phone. |
| `call(handler)` | `kind`, `caller` | A call is `incoming`, `answered` or `ended`. |
| `music(handler)` | `is_playing`, `artist`, `track`, `position`, `length` | The phone's player changes. |
| `weather(handler)` | `temperature`, `condition`, `low`, `high`, `location` | The phone sends a forecast. |
| `message(channel, handler)` | the message's fields | The companion app sends this app a message on its own channel. |
| `http_response(handler)` | `request_id`, `status`, `body` | A reply to an `http_request` effect. |
| `settings(handler)` | the changed keys | The user changes a setting in `settings`. |

When several of the same event are waiting, the runtime delivers only the latest, so a slow handler
never falls behind.

## Widgets

Widgets map onto [rlvgl](https://github.com/softoboros/rlvgl) widgets, which the runtime creates
when a screen opens. Each one takes the common props below plus its own.

### Common props

| Prop | Holds |
|---|---|
| `x`, `y`, `width`, `height` | Position and size in pixels, or `"100%"`, or left out to let the parent's layout decide. |
| `grow` | The share of spare space this widget takes in a `Row` or `Column`. |
| `padding`, `margin` | Space inside and outside the widget. |
| `background`, `color`, `border_color` | Colours, as `"#rrggbb"` or a theme name such as `"accent"`. |
| `border_width`, `radius` | The border and corner rounding. |
| `font` | `small`, `normal`, `large` or `huge`, from the shared library. |
| `align` | `start`, `center` or `end`. |
| `visible` | Whether the widget is shown. `{condition && <Widget/>}` sets it. |
| `animate` | A duration in milliseconds. A bound property that changes slides to its new value instead of jumping. |

### Layout

| Widget | rlvgl | Use |
|---|---|---|
| `Column`, `Row` | container, flex layout | Children one after another, down or across. |
| `Grid` | container, grid layout | Children in rows and columns, such as a keypad. |
| `Stack` | container | Children on top of each other, such as text over an image. |
| `Scroll` | scroll view | Content taller than the screen. |
| `List` | list | A scrolling list of rows, from a fixed-length array. |
| `Pages` | tile view | Full-screen pages the user swipes between. |
| `Tabs` | tab view | Pages with a row of tabs. |

### Showing things

| Widget | rlvgl | Use |
|---|---|---|
| `Label` | label | Text. Long text wraps, or scrolls with `long_mode="scroll"`. |
| `RichText` | span group | Text in several colours or fonts on one line. |
| `Image` | image | A picture from the app or the shared library. |
| `Icon` | image | A shared library icon, drawn in the `color` given. |
| `AnimatedImage` | animated image | A short loop of frames. |
| `Line` | line | Lines between points, such as clock ticks. |
| `Arc` | arc | A ring or part of one, such as a progress ring. |
| `ArcLabel` | arc label | Text along a curve. |
| `Bar` | bar | A straight progress bar. |
| `Spinner` | spinner | Shows that something is in progress. |
| `Led` | led | A dot that is on, off or dimmed. |
| `Chart` | chart | A line or bar chart from a fixed-length array, such as heart rate over a day. |
| `Scale` | scale | Ticks and labels around a dial or along a ruler. |
| `AnalogClock` | clock | Clock hands bound to a time, normally `now`. |
| `Table` | table | Rows and columns of text. |
| `Calendar` | calendar | A month, with chosen days marked. |

### Input

| Widget | rlvgl | Use |
|---|---|---|
| `Button` | button | A tappable button with text or an icon. |
| `ImageButton` | image button | A button drawn as an image. |
| `ClickArea` | click area | An invisible area that takes touches. |
| `Switch` | switch | On or off. |
| `Checkbox` | checkbox | Ticked or not, with a label. |
| `Radio` | radio | One choice from a group. |
| `Slider` | slider | A value dragged along a track. |
| `Roller` | roller | A scrolling wheel of choices, such as hours. |
| `Spinbox` | spinbox | A number with up and down steps. |
| `Dropdown` | dropdown | A choice from a list that opens on tap. |
| `ButtonMatrix` | button matrix | A grid of buttons drawn as one widget, such as a keypad. Cheaper than separate buttons. |
| `TextArea`, `Keyboard` | text area, keyboard | Typing text. The screen is small, so prefer choices where possible. |

### Components from the shared library

These are built from the widgets above and kept in the [shared library](app-runtime.md#the-shared-library),
so they cost an app nothing to include.

| Component | Use |
|---|---|
| `Card` | A rounded panel, the standard block of an app's layout. |
| `Header` | A screen's title row, with an optional icon. |
| `ListRow` | A list entry: icon, title, subtitle and a value or switch on the right. |
| `StatusBar` | The time, battery and connection, as the system shows them. |
| `ConfirmDialog` | A question with two buttons, shown while a signal is true. |
| `NumberPicker`, `TimePicker`, `DurationPicker` | Rollers set up for a number, a time of day or a duration. |
| `Menu` | A list of named actions. |

### Left out

- **Canvas.** It needs a buffer for its pixels, and a full screen is 115 KB, more than the watch's
  RAM. Draw with `Line`, `Arc` and `Image` instead.
- **Window and message box.** `Header` and `ConfirmDialog` cover them.
- **rlvgl's audio meters and motion effects,** which have no use on a watch.

## Effects

Effects are calls a handler makes to ask the system to do something. They run after the handler
finishes without failing, in the order they were called.

### Haptics

| Effect | Does |
|---|---|
| `vibrate(milliseconds)` | Runs the motor once. Capped at 2 seconds. |
| `vibrate_pattern(pattern)` | A named pattern: `tap`, `double_tap`, `notification`, `alarm`, `success` or `failure`. |
| `vibrate_sequence([on, off, on, ...])` | A custom pattern in milliseconds, up to 16 steps. |
| `stop_vibrating()` | Stops any pattern this app started. |

Vibration is skipped during do not disturb unless the app is an alarm, and even then the user can
turn that off.

### Display

| Effect | Does |
|---|---|
| `set_brightness(level)` | Sets the backlight, 1 to 3, until the app closes. |
| `keep_screen_on(on)` | Stops the screen turning off by itself while this app is in front, such as for a flashlight or a map. |
| `wake_screen()` | Turns the screen on and brings this app to the front, such as for an alarm or a timer. |
| `screen_off()` | Turns the screen off now. |

### Navigation

| Effect | Does |
|---|---|
| `go(screen)` | Shows another of this app's screens. |
| `back()` | Returns to the previous screen, or leaves the app from its first one. |
| `open_app(id)` | Opens another installed app. |
| `close()` | Leaves the app for the watch face. |
| `show_toast(text)` | Shows a short message over the screen for two seconds. |

### Notifications

| Effect | Does |
|---|---|
| `notify(title, body)` | Adds a notification on the watch, with the usual vibration. |
| `dismiss_notification(id)` | Removes a notification. |
| `mark_notifications_read()` | Marks every notification read. |

### Phone

| Effect | Does |
|---|---|
| `send_to_phone(channel, data)` | Sends a message to the companion app on this app's channel. |
| `http_request(request_id, method, url, body)` | Asks the phone to make a web request. The reply arrives as an `http_response` event. |
| `music(command)` | `play`, `pause`, `next`, `previous`, `volume_up` or `volume_down`. |
| `call(command)` | `answer`, `reject` or `mute` the current call. |
| `find_phone()` | Makes the phone ring. |

### Sensors

| Effect | Does |
|---|---|
| `measure_heart_rate()` | Takes one reading now. The result arrives through the `heart_rate` subscription and updates `heart_rate`. |
| `start_heart_rate()`, `stop_heart_rate()` | Measures continuously while the app is open, such as during a workout. The runtime stops it when the app closes. |
| `reset_steps()` | Starts today's step count from zero. |

### System

| Effect | Does |
|---|---|
| `set_setting(key, value)` | Changes one of the user's `settings`, such as the step goal. |
| `set_do_not_disturb(on)` | Turns do not disturb on or off. |
| `log(text)` | Writes a line to the debug log the companion app can read. |

Settings and other values that change often are stored on the external flash, never the internal
flash; see the [memory map](memory-map.md).

## Permissions

An app lists what it needs in `define_app({ permissions: [...] })`. The watch asks the user once,
when the app is installed. The compiler checks that every effect and subscription the app uses is
covered, so a missing permission is a compile error, not a surprise on the watch.

| Permission | Covers |
|---|---|
| `heart_rate` | The `heart_rate` subscription and the heart rate effects. |
| `motion` | The `motion` subscription. |
| `notifications` | Reading notifications, and the notification effects. |
| `phone` | `call`, `music`, `find_phone` and their subscriptions. |
| `phone_messages` | `send_to_phone` and the `message` subscription. |
| `internet` | `http_request`. |
| `settings` | `set_setting` and `set_do_not_disturb`. |
| `wake` | `wake_screen`, and vibration during do not disturb. |
| `background` | Subscriptions that run while the app is closed. |

Reading system values, the screen, haptics and navigation need no permission.
