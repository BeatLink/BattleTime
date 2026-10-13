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
| `battery_percent` | `Int` | The battery charge, 0 to 100. It only rises on the charger and only falls off it, stays at 99 while charging and reads 100 once the battery is full. |
| `is_charging` | `Bool` | Whether the watch is on its charger and still charging; false once the battery is full. |
| `battery_millivolts` | `Int` | The battery's voltage in millivolts, smoothed over the last few minutes' readings. |
| `steps_today` | `Int` | Steps since local midnight. |
| `step_history` | object | The system's step history, kept whether or not any app is open: `trip`, the steps since the wearer last called `reset_step_trip()`; `days`, today and the six days before, each with its `total`, read by how many days back it is (`step_history.value.days[1].total` is yesterday); and `hours`, each with its `steps`, read at `days_back * 24 + hour`. `STEP_HISTORY_DAYS` is 7. |
| `heart_rate` | `Int` | The last heart rate measured, in beats per minute, or 0 if none. |
| `heart_rate_measured_at` | `Instant` | When `heart_rate` was measured. |
| `is_phone_connected` | `Bool` | Whether the phone is connected over Bluetooth. |
| `unread_notifications` | `Int` | How many of the kept notifications are unread. |
| `notifications` | list | The latest 5 notifications the watch keeps, from the phone and from apps, newest first: `notifications.value.length`, and `notifications.value[i].id` (for `dismiss_notification`), `.app`, `.title`, `.body` and `.time`, when it arrived. An app's own have negative ids. Needs the `notifications` permission. `apps/notifications` lists them this way. |
| `is_do_not_disturb` | `Bool` | Whether do not disturb is on. |
| `brightness` | `Int` | The backlight level, 1 to 3. |
| `settings` | object | The user's choices apps should follow: `uses_24_hour_clock`, `uses_metric_units`, `step_goal`, `screen_timeout_seconds` (5 to 300), `always_on_screen`, `chimes` (0 off, 1 every hour, 2 every half hour), `allows_updates`, `wake_on_touch` (0 only the button wakes the screen, 1 a tap, 2 a double tap), `alarms_in_do_not_disturb` and `theme`, the preset theme's number in the watch's order (dark, light, ocean, forest, sunset, contrast), or -1 for colours of the user's own. |
| `current_screen` | `Text` | The name of the app's screen on display. |
| `is_heart_rate_running` | `Bool` | Whether the heart rate sensor is measuring for any app. |
| `heart_rate_status` | `Int` | The sensor's state: 0 stopped, 1 no skin, 2 settling, 3 measuring. |
| `is_bluetooth_enabled` | `Bool` | Whether Bluetooth is on, whether or not the phone is connected. It is on again after every restart. |
| `next_alarm` | `Instant \| null` | The next alarm any app has set to ring, or `null` if none is set. In app code an empty optional is always `null`. |
| `current_weather` | object | The last forecast the phone sent: `temperature`, `condition`, `low`, `high`, `sunrise`, `sunset`, `location` and `received_at` (each `null` until the phone sends it), and `forecast`, up to 5 coming days, each with `condition`, `low` and `high`: read `current_weather.value.forecast.length` and `current_weather.value.forecast[day].high`. Reading it needs no permission, so faces need no subscription of their own. |
| `installed_apps` | list | Every app a launcher lists, in order, group by group: `installed_apps.value.length`, and `installed_apps.value[i].name`, `.id` (for `open_app`) and `.icon` (for an `Icon`). `apps/launcher` is a launcher written this way. |
| `launcher_groups` | list | The launcher groups that hold at least one app, in the order the companion set: `.name`, `.icon`, and `.first` and `.count`, the run of `installed_apps` each holds. With no launcher file every app is in one group, `Other`. |
| `launcher_style` | `Text` | How the wearer wants the groups shown: `folders`, `sections`, `pages` or `list` (one list without groups, until the companion sets one). |
| `launcher_view` | `Text` | How the wearer wants a group's apps drawn: `list` or `grid`. |
| `packages` | list | Every installed app and face, on or off, in id order: `.name`, `.id` (for `set_app_enabled`), `.is_face`, `.enabled`, `.memory` (the heap it needs while open, in bytes, as the package manager checks it) and `.refusal`, why turning it the other way would be refused: 0 it would not, 1 it needs more memory than an app can have, 2 the launcher would run out of memory, 3 the launcher stays on, 4 the last watch face stays on, 5 it is the open app. `apps/software` lists them. |
| `app_memory_budget` | `Int` | The most heap an open app may need, in bytes. |
| `current_music` | object | The phone's player: `artist`, `track`, `is_playing`, `position` and `length`. Needs the `phone` permission. |
| `firmware_version` | `Text` | The firmware's version and the commit it was built from, such as `0.1.0+1b19aed`, with a `*` when the build had uncommitted changes. At most 22 characters. |
| `started_at` | `Instant` | When the watch last started, so `now - started_at` is how long it has run. It moves with the clock when the phone sets the time. |
| `free_storage_kilobytes` | `Int` | The space left in the watch's file system, counted when the app opens. |
| `reset_reason` | `Text` | Why the watch last restarted: `Fault at 0x…` or `Last run panicked` when the run before crashed, or else the chip's reset reasons, such as `watchdog`, `reset pin` or `power on`. |
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

Inside an app every swipe is the app's own: the shell takes none, and a right swipe does not go
back. The shell acts on swipes only on the watch face, the launcher, the notifications and quick
settings, and an app opened from the quick-app ring, whose left and right swipes step on round the
ring. A swipe the screen's `onSwipe` ignores, changing nothing and asking for nothing, goes on to
the shell as if the screen had no handler, so an app that uses only up and down still steps round
the ring.

### Side button

Apps never hear the side button. A click goes back one of the app's screens, running the
screen's `onHide` as it leaves, so a screen that must stop something when the wearer leaves it,
such as a ringing alarm, does so there; on the app's first screen the shell goes back a step. On
the watch face a click turns the screen off. A long press or a 2 second hold goes to the face, and
a double click opens the notifications. Holding it for 5 seconds always resets the watch, whatever
the app does. A watch face may still have more screens, such as its settings opened with a long
press on the screen and `go("settings")`.

### Widget values

| Prop | Widgets | Fields |
|---|---|---|
| `onChange` | `Slider`, `Switch`, `Checkbox`, `Radio`, `Roller`, `Spinbox`, `Dropdown`, `TextArea` | `value` |
| `onPageChange` | `Pages`, `Tabs` | `page` |
| `onScrollEnd` | `Scroll`, `List`, `AppList`, `TextFile` | `offset` |

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
| `every(period, handler)` | `time` | Repeatedly, at most every 20 milliseconds. Periods shorter than a minute need `{ only_while_open: true }`, so they never run in the background. |
| `at(instant, handler)` | `time` | Once, at an absolute time. The instant may be a signal, so changing it reschedules. |
| `daily(time_of_day, handler, days?)` | `time` | At a local time of day, `{ hour, minute }`, on the weekdays in `days` (a 7-bit mask, Monday first; every day when left out). `time_of_day` may be a signal, and `null` turns the subscription off, as an alarm switch does. |
| `clock_change(unit, handler)` | `time` | The local `minute`, `hour` or `day` changes, such as a reset at midnight. Also fires when the phone sets the clock. |
| `heart_rate_reading(period, handler)` | `beats_per_minute`, `confidence` | A measurement every `period`. Several apps asking share one measurement. |
| `steps(handler)` | `steps_today` | The step count changes, at most once a minute. |
| `step_goal(handler)` | `steps_today` | The day's step goal is reached. |
| `motion(kind, handler)` | `kind` | The accelerometer sees `wrist_raise`, `shake`, `tap`, `double_tap`, `still` or `moving`. Only `shake` is sent so far, found in software while the app is open and the screen is on. |
| `acceleration(period, handler)` | `x`, `y`, `z` in thousandths of g | Raw readings, such as for a spirit level. Only while the app is open and the screen is on, and at most 10 a second; the accelerometer is read only while an app asks. |
| `battery(kind, handler)` | `percent` | The battery goes `low`, or charging `starts`, `stops` or `completes`. |
| `phone(kind, handler)` | `is_connected` | The phone `connects` or `disconnects`. |
| `notification(handler)` | `id`, `app`, `title`, `body`, `time` | A notification arrives from the phone. The watch keeps up to 24 bytes of `app`, 32 of `title` and 120 of `body`. |
| `call(handler)` | `kind`, `caller` | A call is `incoming`, `answered` or `ended`. |
| `music(handler)` | none | The phone's player changes; read the new state from `current_music`. |
| `weather(handler)` | none | The phone sends a forecast; read it from `current_weather`, coming days included. |
| `navigation(handler)` | none | The phone's navigation app gives the next step; read it from `current_navigation`. |
| `message(channel, handler)` | the message's fields | The companion app sends this app a message on its own channel. |
| `http_response(handler)` | `request_id`, `status`, `body` | A reply to an `http_request` effect. |
| `settings_change(handler)` | `key` | The user changes a setting, from the watch or the phone. `key` is its name as `set_setting` takes it, or `do_not_disturb`; one event comes for each setting changed. |

When several of the same event are waiting, the runtime delivers only the latest, so a slow handler
never falls behind.

## Widgets

Widgets follow LVGL's set, and the app host draws each one from its values when a screen opens.
The second column of each table names the LVGL widget it matches. Each one takes the common props below plus its own.

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

| Widget | LVGL | Use |
|---|---|---|
| `Column`, `Row` | container, flex layout | Children one after another, down or across. |
| `Grid` | container, grid layout | Children in rows and columns, such as a keypad. |
| `Stack` | container | Children on top of each other, such as text over an image. |
| `Scroll` | scroll view | Content longer than its space, down or, with `direction="row"`, across. |
| `List` | list | A scrolling list of rows, from a fixed-length array. A `.map` inside it is one repeated row, so a long list costs an app one row of code, and when the row has a fixed `height` the watch builds only the rows on screen. |
| `Pages` | tile view | Full-screen pages the user swipes between. |
| `Tabs` | tab view | Pages with a row of tabs. |

Nothing adds space by itself: `padding` and `gap` start at 0. Keep 8 pixels between content and
the screen's edges, with `padding={8}` on the screen's outer container, and 6 to 8 pixels between
buttons and other controls, with `gap`. Leave the padding out only for content meant to fill the
screen, such as a game board or a full-width picture. A widget placed with `x` and `y` is measured
from inside its parent's padding.

### Showing things

| Widget | LVGL | Use |
|---|---|---|
| `Label` | label | Text. Long text wraps, or scrolls with `long_mode="scroll"`. |
| `RichText` | span group | Text in several colours or fonts on one line. |
| `Image` | image | A picture from the app or the shared library. |
| `Icon` | image | A shared library icon, drawn in the `color` given. |
| `AnimatedImage` | animated image | A short loop of frames. |
| `Line` | line | Lines between points, such as clock ticks. |
| `Arc` | arc | A ring or part of one, such as a progress ring. Its children are stacked in its middle. |
| `ArcLabel` | arc label | Text along a curve. |
| `Bar` | bar | A straight progress bar. |
| `Spinner` | spinner | Shows that something is in progress. |
| `Led` | led | A dot that is on, off or dimmed. |
| `Chart` | chart | A line or bar chart from one or more fixed-length arrays, such as heart rate over a day. |
| `CellGrid` | drawn by the runtime | A board of coloured cells from one array, such as Life, Snake or 2048. One widget however many cells, so a 400-cell board costs no more UI heap than a label. |
| `BatteryGauge` | drawn by the runtime | A battery's outline filled to `value` percent, with a bolt while `charging`; `BatteryIcon` binds it to the watch's own battery. |
| `StatusRow` | drawn by the runtime | Status icons chosen by `status_flags` (1 notifications, 2 the phone, 4 an alarm), then a battery in `border_color`; `StatusIcons` binds it to the system. |
| `Badge` | drawn by the runtime | An `icon` and a text beside it as one widget, the icon in `recolor` when not the text's colour. |
| `NumberRoller`, `TimeRollers`, `DurationRollers` | drawn by the runtime | Rollers for a whole number from `minimum` to `maximum` padded to `digits`, a time as the minute of the day with `twelve_hour`, or a duration in seconds with `show_hours`; `onChange` sends the new value. The pickers above bind them to signals. |
| `DayButtons` | drawn by the runtime | Seven day buttons over a `value` of day bits; `onChange` sends the days with the tapped one flipped. |
| `Stepper` | drawn by the runtime | A minus button, the value or its `text`, and a plus button, stepping `value` by `step` between `minimum` and `maximum`. |
| `ConfirmBox` | drawn by the runtime | A question and two buttons, its `text` the cancel button's, the confirm button's and the question on lines of their own; `onChange` sends 1 or 0. `ConfirmDialog` binds it to a signal. |
| `AppList` | drawn by the runtime | The installed apps as a scrolling list of rows or a grid, each its icon and name, read from the system by the widget itself, all of them, one launcher group's, or every group under its heading. One widget however many apps, so a launcher is a few bytes. |
| `TextFile` | drawn by the runtime | A text file from the watch's storage, wrapped and scrolled, such as a note the companion app sent. Only the lines near the view are read into RAM, so a long file costs no more than a short one. |
| `TextLines` | drawn by the runtime | The end of a text kept as character codes in an array signal, wrapped to the widget's width at spaces and broken at each new line, showing as many of the last lines as fit. One widget however many lines, for notepads such as morse. |
| `Scale` | scale | Ticks and labels around a dial or along a ruler, with an optional needle at `value`. |
| `AnalogClock` | clock | Clock hands bound to a time, normally `now`. |
| `Table` | table | Rows and columns of text. |
| `Calendar` | calendar | A month, with chosen days marked. |

### Input

| Widget | LVGL | Use |
|---|---|---|
| `Button` | button | A tappable button with text or an icon. |
| `ImageButton` | image button | A button drawn as an image. |
| `ClickArea` | click area | An invisible area that takes touches. Its children are stacked down it, as in a `Column`. |
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
so they will cost an app nothing to include. Until library components exist, the compiler inlines
them into each app; see [Building apps from shared parts](app-runtime.md#building-apps-from-shared-parts).
For a list of entries use a `List` with a `Repeat`, and for the status row the native `StatusIcons`.

| Component | Use |
|---|---|
| `Card` | A rounded panel, the standard block of an app's layout. |
| `Header` | A screen's title row, with an optional icon. |
| `ScrollBar` | One segment per page down the right edge at x 238, the `page` on show in the muted grey and the rest in the surface grey, as wasp-os draws it; nothing for a single page. Put it straight in a `Screen` without padding, which the watch would add to its place, and pad the content beside it by 4 or more. |
| `BatteryIcon` | The battery's charge and whether it is charging, drawn the way the system does, with an optional percentage, as one widget. `single_color` draws it all in `color`, and `show_bolt={false}` hides the charging bolt. |
| `StatusIcons` | The status row as one widget: the notifications icon while any are unread, the phone's while it is connected, the alarm's with `show_alarm` while one is set, then the battery. `icon_size`, `gap`, `color` for the icons, `battery_color`, and `align` to place them across. |
| `StepsBadge`, `HeartRateBadge`, `TemperatureBadge` | Today's steps, the heart rate while it is measured, or the temperature in the wearer's units, beside an icon, as one widget. `icon_size`, `gap`, `icon_color` and `align`; `TemperatureBadge` takes the weather `icon` the face chooses. |
| `ConfirmDialog` | A question with two buttons, shown while a signal is true, as one widget. |
| `NumberPicker`, `TimePicker`, `DurationPicker` | Rollers for a number, a time of day or a duration, writing back to a signal, each one widget. `TimePicker` follows the 12 or 24 hour setting. |
| `WeekdayPicker` | A button for each day of the week, toggling that day in a signal of day bits, Monday in bit 0, as one widget; `columns` buttons a row. |

### Props beyond the common ones

Every angle, in every widget and function, is in degrees clockwise from twelve o'clock.


| Widget | Props |
|---|---|
| `Line` | `points`: a fixed-length array of `{ x, y }`. |
| `Scale` | `ticks`, `major_every`, `labels` for the major ticks, `skip` for tick numbers to leave out (such as under a date window), and `value` and `needle_color` for a gauge. `start_angle` and `end_angle` make a part dial, such as a half gauge. |
| `CellGrid` | `columns`, `rows`, `cell_size`, `cell_gap`, `cell_shape` (`square`, `rounded` or `round`), `cells` (one number per cell, read as a colour from `palette`), `palette` (a fixed array, or a state array to change colours as the app runs), `show_numbers` to draw each cell's value, or with `number_map` the number it maps to, in every cell where that is not 0 (in `font`, or the small font when it does not fit), `number_colors` for the number's colour by cell value, and `onClick` and `onDrag` receiving `{ value }`, the cell's number. |
| `AppList` | `gap` between rows, `padding`, `color` for the icons and names, and `onClick` receiving `{ value }`, the tapped app's place in `installed_apps`. Rows are 52 pixels tall. `first` and `count` show only that run of `installed_apps`, such as one of `launcher_groups`; `sections` shows every app with each group's name above its apps instead; `columns` above 1 lays the apps out as square cells that many across, each a large icon with its name below in the small font. `scrolling` and `scroll_bar` work as on a `Scroll`; the bar's room comes off the rows' or cells' width. |
| `Image` | `width` and `height`, from the common props, for an image that is not square. |
| `Scroll`, `List` | `direction`, `"column"` (the default) to scroll up and down or `"row"` to lay the children out across and scroll left and right; a repeated row across a `List` needs a fixed `width` to be built only while in view. `scrolling`, `"smooth"` (the default) to follow the finger or `"pages"` to settle on a whole page when the finger lifts: a drag of 20 pixels or more turns to the next page, a shorter one goes back, and the last page ends with the content. `scroll_bar={false}` leaves the bar out. All three are written out in the source. While the content is longer than the widget, the watch draws a 2-pixel bar down its right edge, or along its bottom edge when it runs across, and keeps 4 pixels beside the content for it: a smooth bar is a light thumb, as long as the share in view, over a dim track; a paged bar has a segment per page, as wasp-os draws it, the page on show light. |
| `TextFile` | `scrolling` and `scroll_bar`, as on a `Scroll`; `path`, the file to show; `font_path`, a file holding the name of a text font (`small`, `normal`, `medium` or `large`), which wins over `font` when it names one; and `placeholder`, shown in the `muted` colour while the file is missing or empty. All three are written out in the source, not worked out. Tabs and carriage returns show as spaces and bytes that are not UTF-8 as `?`. The file is read when its screen opens, so a file the phone changes shows the next time the screen opens. At most 32,767 pixels of lines are shown, such as about 2,000 lines in the small font; the rest is left out. |
| `TextLines` | `codes`, an array signal of character codes, and `length`, how many of them are in use. A new line at the end starts an empty last line. |
| `Chart` | `series`: up to 3 arrays, each with its colour. |
| `Arc` | `value`, `minimum` and `maximum`. Given a signal, the arc can be dragged and writes back to it, with `onChange`. |
| `Image` | `recolor`: draws a one-colour image, such as a logo, in this colour. |
| `AnalogClock` | `time` (normally `now`), `show_seconds`, `hour_color`, `minute_color`, `second_color`, `hour_length`, `minute_length`, `second_length`, and `hand_style`: `line`, `tapered` or `rounded`. |
| `ButtonMatrix` | `rows`: an array of rows of labels. `onClick` receives `{ value }`, the number of the button counted across the rows. |
| `Roller`, `Dropdown` | `options`, and `value` as the number of the chosen option. |
| `Slider`, `Switch`, `Checkbox`, `Roller`, `Spinbox` | `value` may be a plain value or a signal. Given a signal, the widget writes changes back to it. |

### Shared library images and icons

Besides the system icons, the shared library holds a `pine_logo` image and icons for `alarm`,
`info`, `weather_night_clear`, `weather_night_cloudy` and `none`, the empty icon.

`library_icon(index)`, from `neotime/icons`, gives the icon at that place in the library's own
order, `ICON_NAMES` in `compiler/src/types.rs`, or `none` outside it, for an icon chosen by number
while the app runs, such as one the phone wrote to a file. The order keeps every icon's number, and
new icons go at the end.

### Left out

- **Canvas.** It needs a buffer for its pixels, and a full screen is 115 KB, more than the watch's
  RAM. Draw with `Line`, `Arc` and `Image` instead.
- **Window and message box.** `Header` and `ConfirmDialog` cover them.
- **Audio meters and motion effects,** which have no use on a watch.

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

During do not disturb the watch plays only `vibrate_pattern("alarm")` and `stop_vibrating()`, so
alarms and timers still ring; every other vibration, `notify`'s included, is skipped. With
`settings.alarms_in_do_not_disturb` off, the alarm pattern is skipped too. The watch does not check
the `wake` permission for this yet.

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
| `go(screen)` | Shows another of this app's screens. From an `onSwipe` it slides in the way the swipe went; otherwise it wipes in from the right. |
| `back()` | Returns to the previous screen, or leaves the app from its first one. |
| `open_app(id)` | Opens another installed app; the id may be text made while running, such as `installed_apps.value[i].id`. |
| `close()` | Leaves the app for the watch face. |
| `show_toast(text)` | Shows a short message over the screen for two seconds. |

### Notifications

| Effect | Does |
|---|---|
| `notify(title, body)` | Adds a notification on the watch, with the usual vibration, under the app's name. |
| `dismiss_notification(id)` | Removes a notification. |
| `mark_notifications_read()` | Marks every notification read, so `unread_notifications` is 0. |

### Phone

| Effect | Does |
|---|---|
| `send_to_phone(channel, data)` | Sends the companion app a message on `channel`, such as `send_to_phone("remote", { button: 2 })`. `data` is an object of up to 4 whole numbers; the channel and the keys are written out, each 1 to 32 letters, digits, `_` or `-`. The phone receives `{"t":"app","ch":"remote","d":{"button":2}}`, as [companion-protocol.md](companion-protocol.md#calls-music-weather-and-the-rest-served) describes. |
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
| `reset_step_trip()` | Starts the trip count in `step_history` from zero. Today's count and the history stay. |

### System

| Effect | Does |
|---|---|
| `set_setting(key, value)` | Changes one of the user's settings by name: any key of `settings`, `brightness` (1 to 3, the level the watch returns to), `do_not_disturb` or `bluetooth`. `theme` takes a preset's number. Booleans are 1 or 0, and a value outside its limits is ignored. |
| `set_do_not_disturb(on)` | Turns do not disturb on or off. |
| `log(text)` | Writes a line to the debug log the companion app can read. |
| `set_bluetooth(on)` | Turns Bluetooth on or off; the same as `set_setting("bluetooth", ...)`. A restart turns it on again. |
| `set_app_enabled(id, on)` | Turns an installed app or face on or off. The watch checks the change as the package manager does and does nothing when `packages` gives it a refusal, so show why first. |
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
| `format_fixed(value, places, trim?)` | A number kept in units of a `places`-digit fraction as text: `format_fixed(1250, 3)` is `1.250`. With `trim`, trailing zero decimals are left off. |
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
| `display_hour(instant)` | The hour as the clock shows it: 0 to 23 on a 24 hour clock, else 1 to 12. |
| `minute_of_day(instant)` | Minutes into the local day, 0 to 1439. |
| `is_night(instant)` | Whether the instant is between sunset and sunrise, or from 18:00 to 06:00 until the phone sends them. |
| `steps_since(base)` | Steps taken since today's count was `base`, counting again from today's count after midnight's reset. |
| `temperature_in_unit(celsius)` | Whole degrees in the wearer's units. |
| `digit_of(value, place)` | One decimal digit: `place` 1 for the units, 10 for the tens. |
| `sunday_week_of(instant)` | The week counted from the year's first Sunday, as strftime's `%U` gives; the days before it are week 0. |
| `days_in_year(year)` | 366 in a leap year, else 365. |
| `weather_icon_at(instant)` | The current weather's icon, with the moon in place of the sun at night. |
| `weather_condition_name(condition)` | InfiniTime's one-word name for a condition: Clear, Cloudy, Rain, Showers, Thunder, Snow or Mist. |
| `temperature_color(celsius)` | InfiniTime's colour for a temperature, from blue when freezing to deep orange when hot. |
| `format_time_of_day(hour, minute)` | `07:30` on a 24 hour clock, else `07:30 AM`. |
| `day_mask_name(days)` | A set of days, Monday in bit 0, as Once, Every day, Weekdays, Weekends, or a letter a day such as `M-W----`. |

The compiler works out `sine`, `cosine` and `point_on_circle`; the others run in the library, so
an app carries only a 5 byte call.

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
| `count_of(array, value)` | How many elements equal the value. |
| `nth_index_of(array, value, n)` | Where the `n`th element equal to the value is, counting from 0, or -1. |
| `arrays_equal(first, second)` | Whether two arrays of one length hold the same elements. |
| `line_through(grid, width, cell, value)` | The longest line of the value through the cell, across, down or diagonally, counting the cell as holding it. Lines stop at the edges. |
| `slide_grid(grid, width, direction)` | The grid with its tiles slid that way, equal pairs merging once into a tile one level higher, as in 2048. |
| `slide_points(grid, width, direction)`, `slide_changes(...)`, `can_slide(grid, width)` | What a slide's merges score (two to the power of each new level), whether it changes the grid, and whether any slide would. |
| `count_inversions(array)` | How many pairs are out of order, the earlier one greater; its parity says whether a sliding puzzle can be solved. At most 64 elements. |
| `shuffled(array, first, count)` | The array with the `count` elements from `first` on in a random order. Handlers only. |
| `push_shift(array, value)`, `unshift_pop(array, value)` | The array moved one place along with the value added at the end or the front, dropping the element at the other end: a history of readings or laps. |
| `life_step(grid, width, live_value)` | The next generation of the Game of Life on a grid that wraps at its edges; each live cell holds `live_value`. At most 16 wide. |
| `file_bytes(path, length)` | The first `length` bytes of a file on the watch, each 0 to 255, with 0 past the file's end or when it is missing, such as a file the companion app wrote. `path` is written out and `length` known when compiling. The file is read when the handler runs, so `onOpen` sees what the phone last wrote. The browser preview reads it from `preview_files` as UTF-8. |
| `array.map(...)` | A new array of the same length. Allowed in views, `computed` and handlers. |

Loops over fixed-length arrays are allowed in `computed` and handlers as well as views.

The interpreter answers the functions from `count_of` down natively, so they cost a few bytes of
the image where a loop costs dozens, and run far faster; `indexOf` and `includes` on an array signal
do the same. Their arrays must be signals of numbers, such as `board.value`, or parameters given one.
`slide_grid`, `life_step`, `push_shift`, `unshift_pop`, `shuffled` and `file_bytes` give a whole array, so they may only be assigned to an array signal of
the same length in a handler, as in `board.value = life_step(board.value, 16, 1)` or
`layout.value = file_bytes("/remote/buttons", 11)`. A grid is
numbered across each row from the top left; a slide's lines are at most 16 long.

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
| `settings` | `set_setting`, `set_do_not_disturb`, `set_bluetooth` and `set_app_enabled`. |
| `wake` | `wake_screen`, and vibration during do not disturb. |
| `background` | Subscriptions that run while the app is closed. |

Reading system values and files, the screen, haptics and navigation need no permission.
