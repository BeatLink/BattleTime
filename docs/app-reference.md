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
| `is_heart_rate_running` | `Bool` | Whether the heart rate sensor is measuring for any app. |
| `heart_rate_status` | `Int` | The sensor's state: 0 stopped, 1 no skin, 2 settling, 3 measuring. |
| `is_bluetooth_enabled` | `Bool` | Whether Bluetooth is on, whether or not the phone is connected. |
| `next_alarm` | `Instant \| null` | The next alarm any app has set to ring, or `null` if none is set. In app code an empty optional is always `null`. |
| `current_weather` | object | The last forecast the phone sent: `temperature`, `condition`, `low`, `high`, `sunrise`, `sunset`, `location` and `received_at` (each `null` until the phone sends it). Reading it needs no permission, so faces need no subscription of their own. |
| `current_music` | object | The phone's player: `artist`, `track`, `is_playing`, `position` and `length`. Needs the `phone` permission. |
| `current_navigation` | object | The phone's navigation: `turn`, `instruction` (text), `distance` in metres and `progress_percent`. Needs the `phone` permission. |

The system keeps the latest weather, music and navigation, including their text, and keeps them
through a reboot, so an app shows them by reading these values rather than copying event text into
its own state. The `weather`, `music` and `navigation` subscriptions are for reacting when they change.

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

Without an `onSwipe`, swipes keep their system meaning: right goes back and down opens the quick
settings. A screen with an `onSwipe` receives every swipe instead, and the system meanings no
longer apply, so the handler calls `back()` itself if right should still go back. An empty
`onSwipe` therefore blocks swipes, as an alarm's ringing screen needs.

### Side button

| Prop | Fields | When |
|---|---|---|
| `onButton` | `kind`: `press` or `long_press` | The side button, set on a screen. |

Without a handler the button goes back, and from the watch face it turns the screen off. Watch faces
cannot take the button. A watch face may still have more screens, such as its settings opened with
a long press and `go("settings")`. Holding it for 5 seconds always resets the watch, whatever the app does.

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
while the app is closed; see [Background work](app-runtime.md#background-work). Any subscription
takes a last argument `{ only_while_open: true }`, which needs no `background` permission and
stops when the app closes, such as for shaking to roll dice.

An `at` time that has already passed, for example because the watch was off, fires once at once.
A handler run in the background may vibrate and notify without the `wake` permission; it needs
`wake` only to turn the screen on or to vibrate during do not disturb.

| Subscription | Fields | When |
|---|---|---|
| `every(period, handler)` | `time` | Repeatedly. Periods shorter than a minute run only while the app is open. |
| `at(instant, handler)` | `time` | Once, at an absolute time. The instant may be a signal, so changing it reschedules. |
| `daily(time_of_day, handler, days?)` | `time` | At a local time of day, `{ hour, minute }`, on the weekdays in `days` (a 7-bit mask, Monday first; every day when left out). `time_of_day` may be a signal, and `null` turns the subscription off, as an alarm switch does. |
| `clock_change(unit, handler)` | `time` | The local `minute`, `hour` or `day` changes, such as a reset at midnight. Also fires when the phone sets the clock. |
| `heart_rate_reading(period, handler)` | `beats_per_minute`, `confidence` | A measurement every `period`. Several apps asking share one measurement. |
| `steps(handler)` | `steps_today` | The step count changes, at most once a minute. |
| `step_goal(handler)` | `steps_today` | The day's step goal is reached. |
| `motion(kind, handler)` | `kind` | The accelerometer sees `wrist_raise`, `shake`, `tap`, `double_tap`, `still` or `moving`. Which ones exist depends on the chip; the BMA425 has them all. |
| `acceleration(period, handler)` | `x`, `y`, `z` in thousandths of g | Raw readings, such as for a spirit level. Only while the app is open, and at most 10 a second. |
| `battery(kind, handler)` | `percent` | The battery goes `low`, or charging `starts`, `stops` or `completes`. |
| `phone(kind, handler)` | `is_connected` | The phone `connects` or `disconnects`. |
| `notification(handler)` | `id`, `app`, `title`, `body`, `time` | A notification arrives from the phone. |
| `call(handler)` | `kind`, `caller` | A call is `incoming`, `answered` or `ended`. |
| `music(handler)` | `is_playing`, `artist`, `track`, `position`, `length` | The phone's player changes. |
| `weather(handler)` | `temperature`, `condition`, `low`, `high`, `location`, `days` | The phone sends a forecast. `days` holds up to 5 coming days, each with `condition`, `low` and `high`. |
| `navigation(handler)` | `turn`, `instruction`, `distance` (metres), `progress_percent` | The phone's navigation app gives the next step. |
| `message(channel, handler)` | the message's fields | The companion app sends this app a message on its own channel. |
| `http_response(handler)` | `request_id`, `status`, `body` | A reply to an `http_request` effect. |
| `settings_change(handler)` | the changed keys | The user changes a setting in `settings`. |

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
| `background`, `color`, `border_color` | Colours, as `"#rrggbb"` or a theme name: `background`, `surface`, `text`, `muted`, `accent`, `success`, `warning` or `danger`. A theme name follows the user's theme, and changes with it without the app doing anything. |
| `border_width`, `radius` | The border and corner rounding. |
| `font` | `small`, `normal`, `medium` (about 30 px), `large`, `huge` or `giant` (about 120 px, digits and `:` only), or a display font: `seven_segment` (about 76 px, digits only), `dot_matrix` (about 40 px, digits and capital letters A to Z) or `condensed` (about 48 px, digits only). Each display font has one size; another size would be a new font name. All are in the shared library. |
| `align` | `start`, `center` or `end`. |
| `visible` | Whether the widget is shown. `{condition && <Widget/>}` sets it. |
| `animate` | A duration in milliseconds. A bound property that changes slides to its new value instead of jumping. |
| `opacity` | 0 to 255. |
| `gap` | On a container, the space in pixels between its children. |
| `justify`, `align_items` | On a container, where its children sit along and across it: `start`, `center`, `end` or `space_between`. |
| `disabled` | On an input, shows it greyed out and ignores touches. |

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
| `Chart` | chart | A line or bar chart from one or more fixed-length arrays, such as heart rate over a day. |
| `CellGrid` | drawn by the runtime | A board of coloured cells from one array, such as Life, Snake or 2048. One widget however many cells, so a 400-cell board costs no more UI heap than a label. |
| `Scale` | scale | Ticks and labels around a dial or along a ruler, with an optional needle at `value`. |
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
| `BatteryIcon` | The battery's charge and whether it is charging, drawn the way the system does, with an optional percentage. `single_color` draws it all in `color`, and `show_bolt={false}` hides the charging bolt. |
| `ConfirmDialog` | A question with two buttons, shown while a signal is true. |
| `NumberPicker`, `TimePicker`, `DurationPicker` | Rollers set up for a number, a time of day or a duration. |
| `Menu` | A list of named actions. |

### Props beyond the common ones

Every angle, in every widget and function, is in degrees clockwise from twelve o'clock.


| Widget | Props |
|---|---|
| `Line` | `points`: a fixed-length array of `{ x, y }`. |
| `Scale` | `ticks`, `major_every`, `labels` for the major ticks, `skip` for tick numbers to leave out (such as under a date window), and `value` and `needle_color` for a gauge. `start_angle` and `end_angle` make a part dial, such as a half gauge. |
| `CellGrid` | `columns`, `rows`, `cell_size`, `cell_gap`, `cell_shape` (`square`, `rounded` or `round`), `cells` (one number per cell, read as a colour from `palette`), `palette` (a fixed array, or a state array to change colours as the app runs), and `onClick` and `onDrag` receiving `{ value }`, the cell's number. |
| `Image` | `width` and `height`, from the common props, for an image that is not square. |
| `Chart` | `series`: up to 3 arrays, each with its colour. |
| `Arc` | `value`, `minimum` and `maximum`. Given a signal, the arc can be dragged and writes back to it, with `onChange`. |
| `Image` | `recolor`: draws a one-colour image, such as a logo, in this colour. |
| `AnalogClock` | `time` (normally `now`), `show_seconds`, `hour_color`, `minute_color`, `second_color`, `hour_length`, `minute_length`, `second_length`, and `hand_style`: `line`, `tapered` or `rounded`. |
| `ButtonMatrix` | `rows`: an array of rows of labels. `onClick` receives `{ value }`, the number of the button counted across the rows. |
| `Roller`, `Dropdown` | `options`, and `value` as the number of the chosen option. |
| `Slider`, `Switch`, `Checkbox`, `Roller`, `Spinbox` | `value` may be a plain value or a signal. Given a signal, the widget writes changes back to it. |

### Shared library images and icons

Besides the system icons, the shared library holds a `pine_logo` image and icons for `alarm`,
`weather_night_clear`, `weather_night_cloudy` and `none`, the empty icon.

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
| `vibrate_pattern(pattern)` | A named pattern: `tap`, `double_tap`, `notification`, `alarm`, `success` or `failure`. `alarm` repeats until `stop_vibrating()`, the app closes, or a minute passes. |
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
| `music_control(command)` | `play`, `pause`, `next`, `previous`, `volume_up` or `volume_down`. |
| `call_control(command)` | `answer`, `reject` or `mute` the current call. |
| `find_phone(on)` | Makes the phone ring, or stops it. |
| `request_music_state()` | Asks the phone to send its player's state now, which arrives through the `music` subscription. |
| `request_navigation_state()` | Asks the phone for the current navigation step, which arrives through the `navigation` subscription. |

### Sensors

| Effect | Does |
|---|---|
| `measure_heart_rate()` | Takes one reading now. The result arrives through the `heart_rate` subscription and updates `heart_rate`. |
| `start_heart_rate()`, `stop_heart_rate()` | Measures continuously while the app is open, such as during a workout. The runtime stops it when the app closes. |

The sensor runs while anything needs it: an app that called `start_heart_rate()` and has not
stopped or closed, a `heart_rate_reading` subscription when its next reading is due, or a
`measure_heart_rate()` call. `stop_heart_rate()` ends only that app's own continuous measuring;
subscriptions keep their schedules. A subscription period under a minute needs
`{ only_while_open: true }`, so a background app cannot keep the sensor on all the time.
| `reset_steps()` | Starts today's step count from zero. |

### System

| Effect | Does |
|---|---|
| `set_setting(key, value)` | Changes one of the user's `settings`, such as the step goal. |
| `set_do_not_disturb(on)` | Turns do not disturb on or off. |
| `log(text)` | Writes a line to the debug log the companion app can read. |
| `set_bluetooth(on)` | Turns Bluetooth on or off. |
| `set_heart_rate_led(on, brightness, period)` | Drives the heart rate sensor's LED directly, such as for a beacon. `brightness` is the drive level, 0 to 3, and `period` the flash period in milliseconds, 0 for steady. The runtime turns it off when the app closes, even with the `background` permission, to save the battery. |

Settings and other values that change often are stored on the external flash, never the internal
flash; see the [memory map](memory-map.md).

## Functions

These are plain functions, usable in views, `computed` and handlers unless marked otherwise.

### Numbers

Every number is a 64-bit whole number, from about -9.2 × 10¹⁸ to 9.2 × 10¹⁸; there are no
fractions. For decimals, keep a fixed number of places, such as millionths, in a whole number. The
compiler accepts `Math.floor`, `Math.trunc`, `Math.round`, `Math.min`, `Math.max` and `Math.abs`.
A bare `/` is rejected, because JavaScript would give a fraction; write `Math.trunc(a / b)` or
`Math.floor(a / b)` to say which way it rounds. Overflow and division by zero fail the call.

The bitwise operators `&`, `|`, `^`, `~`, `<<` and `>>` are accepted, such as for day masks. A shift
by less than 0 or more than 63 places fails the call.

| Function | Returns |
|---|---|
| `random_int(lowest, highest)` | A random whole number between the two, both included. Handlers only, so a view always shows the same thing for the same state. |
| `clamp(value, lowest, highest)` | `value` kept between the two. |

### Text

Template literals, such as `` `${steps} steps` ``, join text and numbers.

| Function | Returns |
|---|---|
| `format_number(number, minimum_digits?)` | The number as text, padded with zeros. |
| `format_duration(duration, pattern)` | The duration in a pattern, such as `"mm:ss"` or `"h:mm:ss"`. |
| `format_time(instant, pattern)` | The local time in a pattern, such as `"dddd D MMMM"`, or one of the named patterns below. |
| `format_temperature(celsius)` | The temperature in the user's units, with its sign. |
| `to_upper(text)` | The text in capitals, such as `MON` or `OCT` from a pattern's names. |
| `pad_start(text, width, character?)` | The text with spaces, or the character given, added in front up to `width`. |
| `text_length(text)` | The number of characters. |
| `slice_text(text, start, end?)` | Part of the text, counted in characters. |
| `join(texts, separator)` | The texts of a fixed-length array joined together. |
| `text_from_codes(codes, length)` | Text made from the first `length` character codes of an array, for text built while the app runs. |

### Format patterns

A pattern is always a literal string, so the compiler checks it, and the watch checks it again at
install. Text inside `[brackets]` is copied as it is, and so is any character that is not a letter.
Any other letter is an error.

| Time token | Shows |
|---|---|
| `YYYY` | The year, such as `2026`. |
| `MMMM`, `MMM` | The month's name, such as `October` or `Oct`. |
| `MM`, `M` | The month's number, as `01` to `12` or `1` to `12`. |
| `DD`, `D` | The day of the month, as `01` to `31` or `1` to `31`. |
| `dddd`, `ddd` | The weekday's name, such as `Sunday` or `Sun`. |
| `HH`, `H` | The hour on a 24 hour clock, as `00` to `23` or `0` to `23`. |
| `hh`, `h` | The hour on a 12 hour clock, as `01` to `12` or `1` to `12`. |
| `mm`, `m` | The minute. |
| `ss`, `s` | The second. |
| `a`, `A` | `am` or `pm`, `AM` or `PM`. |

The named time patterns follow the user's 12 or 24 hour setting: `"clock"` is `HH:mm` or `h:mm`,
`"clock_with_seconds"` adds `:ss`, and `"date"` is `YYYY-MM-DD`.

| Duration token | Shows |
|---|---|
| `hh`, `h` | Hours. |
| `mm`, `m` | Minutes. |
| `ss`, `s` | Seconds. |
| `cc` | Hundredths of a second, always two digits. |

The largest unit in a duration pattern shows the whole amount, so `mm:ss` shows 75 minutes as
`75:03`, and `h:mm:ss` shows it as `1:15:03`.

A binding redraws as often as the smallest unit its pattern shows: every second with `ss`, ten
times a second with `cc`, and otherwise once a minute.

### Time

All in local time, using `utc_offset`.

| Function | Returns |
|---|---|
| `year_of`, `month_of`, `day_of`, `hour_of`, `minute_of`, `second_of` | That part of an instant. |
| `weekday_of(instant)` | 0 to 6, Monday first. |
| `start_of_minute(instant)`, `start_of_hour(instant)`, `start_of_day(instant)` | The start of that local minute, hour or day. A view that shows only these redraws once a minute, not once a second, such as an analog clock without a second hand. |
| `next_time_of_day(after, time_of_day, days?)` | The first instant after `after` at that local time, on one of `days`. |
| `day_of_year(instant)` | 1 to 366. |
| `iso_week_of(instant)` | The ISO 8601 week number, 1 to 53. |

### Shared library

From `neotime/lib`; they live in the [shared library](app-runtime.md#the-shared-library).

| Function | Returns |
|---|---|
| `sine(degrees)`, `cosine(degrees)` | In thousandths, from a table. |
| `point_on_circle(center_x, center_y, radius, degrees)` | `{ x, y }`, with 0 degrees at the top and angles going clockwise, as on a clock face. |

### Records and arrays

State may hold objects of a fixed shape, and fixed-length arrays of numbers or of such objects,
such as a list of alarms `{ hour, minute, days, enabled }`. The compiler stores each field in a
state slot of its own. A handler may write one element, as in `board.value[cell] = 2`; the write is
applied with the handler's other writes when it finishes, and later reads in the same handler see it.
The same holds for single values: after `count.value = 3`, reading `count.value` in that handler gives 3.

A field of a record in an array can be written the same way, as in `alarms.value[i].enabled = true`.
The compiler keeps each field of such an array as an array of its own, so this is one element write.

A text value held in state can only be one of the app's own literal strings. Text made while the app
runs, such as typed or joined text, cannot be stored as text. Keep its characters as an array of
character codes instead, and show it with `text_from_codes(codes, length)`; at 1 byte per code for
plain ASCII, 96 characters take 96 bytes.

Assigning a whole array, as in `board.value = CELLS.map((cell) => next_state(board.value, cell))`,
follows JavaScript: the right side reads the old array throughout. The compiler builds the result in
a hidden array of its own and then copies it in, so that hidden array counts toward the limits
below. A local holds one value, so an array cannot be kept in a handler's `const` or `let`.

An app's state is at most 2 KB, and one array at most 1 KB. Each array stores its elements in 1, 2,
4 or 8 bytes, whichever the compiler finds fits, so a 400-cell board of small numbers takes
400 bytes. One handler may change at most 16 variables and 1 KB of state.

| Function | Returns |
|---|---|
| `range(count)` | `0` to `count - 1`, where `count` is known when the app is compiled. |
| `array.includes(value)`, `array.indexOf(value)` | Whether the value is there, and where (or -1). |
| `count_where(array, test)` | How many elements pass the test. |
| `array.map(...)` | A new array of the same length. Allowed in views, `computed` and handlers. |

Loops over fixed-length arrays are allowed in `computed` and handlers as well as views.

### Control flow

An early `return` is allowed in any function, and `break` and `continue` in any loop. A call that loops runs at most 200,000 steps, which
should be roughly a fifth of a second; a game search should stop well inside that, or split its work across
several calls.

## Permissions

An app lists what it needs in `define_app({ permissions: [...] })`. The watch asks the user once,
when the app is installed. The compiler checks that every effect and subscription the app uses is
covered, so a missing permission is a compile error, not a surprise on the watch.

| Permission | Covers |
|---|---|
| `heart_rate` | The `heart_rate_reading` subscription, the heart rate effects and `set_heart_rate_led`. |
| `motion` | The `motion` and `acceleration` subscriptions. |
| `notifications` | Reading notifications, and the notification effects. |
| `phone` | `call_control`, `music_control`, `find_phone`, and the `call`, `music` and `navigation` subscriptions. |
| `phone_messages` | `send_to_phone` and the `message` subscription. |
| `internet` | `http_request`. |
| `settings` | `set_setting`, `set_do_not_disturb` and `set_bluetooth`. |
| `wake` | `wake_screen`, and vibration during do not disturb. |
| `background` | Subscriptions that run while the app is closed. |

Reading system values, the screen, haptics and navigation need no permission.
