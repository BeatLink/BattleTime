# App reference

This page lists what a BattleTime app can declare, read, answer, show and ask for.
[writing-apps.md](writing-apps.md) shows how an app is written, [compiler.md](compiler.md) the
language the compiler takes, and [app-runtime.md](app-runtime.md) how the watch runs it.

Every name comes from the `battletime` package: values, events, effects and functions from `battletime`,
widgets from `battletime/widgets`, ready-made components from `battletime/components`, icons from
`battletime/icons` and shared library functions from `battletime/lib`.

## Declaring an app

`export default define_app({...})` says who the app is:

| Field | Holds |
|---|---|
| `id` | The app's id, which names its folder: up to 32 bytes. The watch takes letters, digits, `-` and `_`; repository apps use lower-case words joined by hyphens, such as `four-in-a-row`. |
| `name` | The name the launcher shows. |
| `icon` | A shared library icon, such as `icons.timer`, or a PNG beside the source, such as `"./icon.png"`. |
| `kind` | `"app"`, the default, or `"face"`; a watch face cannot take the button. |
| `group` | The launcher group the app starts in, such as `"games"`: up to 16 letters, digits, `-` and `_`. [apps.md](apps.md) lists the usual ones. |
| `screens` | One component per screen, `main` first. `go("name")` moves between them. |

`define_app` takes no list of permissions: the compiler works them out (see
[Permissions](#permissions)).

| State | Is |
|---|---|
| `signal(start)` | A value that lasts while the app is open. |
| `persisted("key", start)` | A value kept across restarts, under its key. |
| `useSignal(start)` | A value for that place in a component. |
| `computed(() => ...)` | A value worked out from others. |

State may hold numbers, booleans, `Instant`, `Duration`, one of the app's own literal strings,
records of a fixed shape and fixed-length arrays of those; see [Records and arrays](#records-and-arrays).

| Constant | Value |
|---|---|
| `SECOND`, `MINUTE`, `HOUR`, `DAY` | Durations in milliseconds. |
| `EVERY_DAY` | `0b1111111`, a day mask with every day set. |
| `STEP_HISTORY_DAYS` | 7, the days `step_history` keeps. |

## System values

System values are read-only signals the runtime keeps current. Reading one in a view or a
`computed` makes that binding depend on it, so it redraws when the value changes and at no other
time. Reading them needs no permission unless the table says so.

### Time, battery and activity

| Value | Type | Holds |
|---|---|---|
| `now` | `Instant` | The current time. A binding that reads it redraws as often as its format shows, such as once a minute for `HH:mm`. |
| `utc_offset` | `Duration` | The local time zone's offset, set by the phone. |
| `battery_percent` | `Int` | The charge, 0 to 100. It only rises on the charger and only falls off it, stays at 99 while charging and reads 100 once full. |
| `is_charging` | `Bool` | Whether the watch is on its charger and still charging. |
| `battery_millivolts` | `Int` | The battery's voltage, smoothed over the last few minutes. |
| `steps_today` | `Int` | Steps since local midnight. |
| `step_history` | object | `trip`, the steps since `reset_step_trip()`; `days`, today and the six days before, each with its `total`, read by how many days back it is (`step_history.value.days[1].total` is yesterday); and `hours`, each with its `steps`, read at `days_back * 24 + hour`. |
| `heart_rate` | `Int` | The last heart rate measured, in beats per minute, or 0. |
| `heart_rate_measured_at` | `Instant` | When `heart_rate` was measured. |
| `heart_rate_status` | `Int` | The sensor's state: 0 stopped, 1 no skin, 2 settling, 3 measuring. |
| `is_heart_rate_running` | `Bool` | Whether the sensor is measuring for any app. |
| `shake_speed` | `Int` | How fast the wrist shakes now, 0 to about 4095, as InfiniTime counts it. Kept current only while the open screen shows it. |
| `next_alarm` | `Instant \| null` | When the alarm app rings next while closed, or `null`. |

### Phone and notifications

| Value | Type | Holds |
|---|---|---|
| `is_phone_connected` | `Bool` | Whether the phone is connected. |
| `is_bluetooth_enabled` | `Bool` | Whether Bluetooth is on. It is on again after every restart. |
| `unread_notifications` | `Int` | How many kept notifications are unread. |
| `notifications` | list | The latest 5 notifications, newest first; needs `notifications`. Each has `id` (for `dismiss_notification`; an app's own are negative), `app`, `title`, `body`, `time`, `icon` and `color`. |
| `current_call` | object | The latest call: `caller`, its name or number, and `state`, `incoming`, `answered` or `ended`. Needs `phone`. |
| `current_music` | object | The phone's player: `artist`, `track`, `is_playing`, `position` and `length`. Needs `phone`. |
| `current_navigation` | object | `turn`, `instruction`, `distance` in metres and `progress_percent`. Needs `phone`. |
| `current_weather` | object | `temperature`, `condition`, `low`, `high`, `sunrise`, `sunset`, `location` and `received_at`, each `null` until the phone sends it, and `forecast`, up to 5 coming days of `condition`, `low` and `high`. Temperatures are whole degrees Celsius. |

The watch picks each notification's `icon` and `color` from its `app`:

| `app` | Icon | Colour |
|---|---|---|
| `Call`, `Phone` | `phone` | `success` |
| `Missed call`, or `Phone` with the body `Missed call` | `phone_missed` | `danger` |
| `SMS` | `sms` | blue `#3078f8` |
| `Messages` | `chat` | blue |
| `Voicemail` | `voicemail` | blue |
| `Email` | `notification` | `warning` |
| `Schedule`, `Calendar` | `calendar` | purple `#a850f8` |
| `News` | `news` | `accent` |
| `Alert` | `bell` | `accent` |
| Any other | `notification` | `accent` |

The watch keeps up to 24 bytes of a notification's `app`, 32 of its `title` and 120 of its `body`.
The latest music, weather and navigation are kept in RAM, so an app shows them by reading these
values; the `music`, `weather` and `navigation` events are for reacting when they change.

### Settings

`settings` holds the user's choices, which apps read and change with `set_setting`:

| Setting | Holds |
|---|---|
| `uses_24_hour_clock`, `uses_metric_units` | Booleans. |
| `step_goal` | The day's step goal. |
| `screen_timeout_seconds` | 5 to 300. |
| `always_on_screen` | Whether the screen dims instead of turning off. |
| `chimes` | 0 off, 1 every hour, 2 every half hour. |
| `theme` | The preset theme's number (dark, light, ocean, forest, sunset, contrast), or -1 for the user's own colours. |
| `allows_updates` | 0 off, 1 on, 2 on until the next restart. |
| `wake_on_touch` | 0 only the button wakes the screen, 1 a tap, 2 a double tap. |
| `alarms_in_do_not_disturb` | Whether alarms and timers still ring in do not disturb. |
| `heart_rate_interval` | 0 off, 1 continuous, then every 30 seconds, minute, 5, 10 or 30 minutes. |
| `raise_to_wake`, `lower_to_sleep`, `shake_to_wake` | Wake gestures. |
| `shake_threshold` | 0 to 4095, the `shake_speed` a shake must pass to wake the screen. |
| `sleep_mode` | Do not disturb that also turns off every wake gesture and the always-on screen. |
| `watch_face` | The number of the face shown, made from its id as [companion-protocol.md](companion-protocol.md) says, or 0 with no face on. |
| `gadgetbridge_name` | Whether the watch advertises as InfiniTime for Gadgetbridge. |

| Value | Type | Holds |
|---|---|---|
| `is_do_not_disturb` | `Bool` | Whether do not disturb is on. |
| `brightness` | `Int` | The backlight level, 1 to 3. |
| `watch_face_name` | `Text` | The name of the face shown, or nothing. |

### Apps and storage

| Value | Type | Holds |
|---|---|---|
| `installed_apps` | list | Every app a launcher lists, group by group: `.name`, `.id` (for `open_app`) and `.icon`. |
| `launcher_groups` | list | The groups holding at least one app: `.name`, `.icon`, and `.first` and `.count`, the run of `installed_apps` each holds. With no launcher file every app is in one group, `Other`. |
| `launcher_style` | `Text` | `folders`, `sections`, `pages` or `list`. |
| `launcher_view` | `Text` | `list` or `grid`. |
| `packages` | list | Every installed app and face, in id order: `.name`, `.id` (for `set_app_enabled`), `.is_face`, `.enabled`, `.memory`, the heap it needs, and `.refusal`, why turning it the other way would be refused: 0 it would not, 1 it needs more memory than an app can have, 2 the launcher would run out of memory, 3 the launcher stays on, 4 the last watch face stays on, 5 it is the open app. |
| `app_memory_budget` | `Int` | The most heap an open app may need, in bytes. |
| `current_folder` | object | The folder `open_folder` last listed: `path`, such as `/apps`, and `entries`, up to 64 of `.name`, `.size` in bytes and `.is_folder`, folders first. A folder's size is the sum of the files inside it. |
| `free_storage_kilobytes` | `Int` | The space left in the file system, counted when the app opens. |

### The watch itself

| Value | Type | Holds |
|---|---|---|
| `firmware_version` | `Text` | Such as `0.1.0+1b19aed`, with a `*` for a build with uncommitted changes. |
| `firmware_build_date` | `Text` | The UTC day the firmware was built. |
| `firmware_state` | `Int` | 0 confirmed, 1 on trial after an update, 2 rolled back to. |
| `started_at` | `Instant` | When the watch last started. |
| `reset_reason` | `Text` | Why it last restarted: `Fault at 0x…`, `Last run panicked`, or the chip's reasons, such as `watchdog`. |
| `bootloader_version`, `bootloader_build_date` | `Text` | From the bootloader's version record, or `""`. Looked up the first time an app reads it. |
| `bootloader_crc` | `Int \| null` | The bootloader's CRC-32, or `null` when unreadable. |
| `bluetooth_address` | `Int` | The advertised address as one 48-bit number. |
| `flash_id` | `Int` | The external flash's JEDEC ID, `0x0B4016` for the XT25F32B, or 0. |
| `touch_chip_id`, `touch_firmware_version` | `Int` | `0xB4` for a CST816S, or 0. |
| `accelerometer_chip_id` | `Int` | `0x11` for a BMA421, `0x13` for a BMA425, or 0. |
| `free_heap_bytes`, `least_free_heap_bytes` | `Int` | The heap's free bytes when the app opened, and the fewest since start. |
| `allocation_failures` | `Int` | Allocations the heap refused since start. |
| `stack_overflows` | `Int` | Stack overflows into the guard, counted until the power goes. |
| `tasks` | list | The firmware's tasks: `.name`, `.runs` and `.busy_milliseconds`. |

`current_screen`, the name of the screen on display, exists only in the browser preview; the
compiler refuses it, so keep a screen's name in a signal.

## Events

Events call an app's handlers, which receive the event's fields as their argument. Handlers run one
at a time, to the end.

### Touch

Touch events belong to the widget touched. A widget with no handler for an event lets it pass to
the widget behind it, and then to the screen.

| Prop | Fields | When |
|---|---|---|
| `onClick` | `x`, `y` | A short tap. |
| `onLongPress` | `x`, `y` | A finger held still for half a second. |
| `onDoubleClick` | `x`, `y` | Two taps close together, as the touch panel reports them. |
| `onPress`, `onRelease` | `x`, `y` | A finger landing and lifting. |
| `onDrag` | `x`, `y`, `start_x`, `start_y` | A finger moving while down. |
| `onSwipe` | `direction` | A slide `up`, `down`, `left` or `right`, set on a screen. |

A stroke of 40 pixels is a swipe while the finger is still down, and a quick flick of 20 pixels is
one as it lifts. A touch on a `CellGrid`, `ButtonMatrix` or `AppList` carries `value`, the cell,
button or app touched.

Inside an app every swipe is the app's own, and a right swipe does not go back. A swipe the
screen's `onSwipe` ignores, changing nothing and asking for nothing, goes on to the shell, so an app
opened from the quick-app ring that uses only up and down still steps round the ring.

### Side button

Apps never hear the side button. A click goes back one of the app's screens, running the screen's
`onHide` as it leaves; on the app's first screen the shell goes back a step. On the watch face a
click turns the screen off. A long press or a 2 second hold goes to the face, and a double click
opens the notifications. A 5 second hold always resets the watch.

### Widget values and screens

| Prop | On | Fields |
|---|---|---|
| `onChange` | `Slider`, `Switch`, `Checkbox`, `Roller`, and the rollers, `DayButtons`, `Stepper` and `ConfirmBox` | `value` |
| `onPageChange` | `Pages` | `page` |
| `onScrollEnd` | `Scroll`, `List`, `AppList`, `TextFile` | `offset`, once the scroll, or a flung list's glide, ends |
| `onShow`, `onHide` | a `Screen` | none; the screen comes into view or leaves it |

A widget whose value is a signal, such as `<Slider value={volume} />` without `.value`, writes the
new value back itself, so `onChange` is only needed for anything else to happen.

### System events

`on(event, handler, options)`, at the top of the file and before `define_app`, connects a handler
to an event. Each becomes one of the image's subscriptions, so `on` inside a handler is an error.
Events other than the app's own life call their handler even while the app is closed, unless given
`while_open: true`.

```tsx
on("open", start);
on("tick", update, { every: MINUTE });
on("time", ring, { at: next_ring_at });
on("shake", roll, { while_open: true, at_most_every: SECOND });
```

| Option | Means |
|---|---|
| `while_open` | `true` makes it come only while the app is open, which needs no `background` permission. Written out. |
| `every` | How often `tick`, `heart_rate` and `acceleration` come: milliseconds such as `MINUTE`, or a period such as `{ seconds: 1 }` (`milliseconds`, `seconds`, `minutes`, `hours`), known when compiling. |
| `at` | When `time` comes, an `Instant` or a signal holding one; or the time of day `daily` comes, `{ hour, minute }` or a signal holding one. |
| `days` | The weekdays `daily` comes on, a constant 7-bit mask with Monday in bit 0; every day when left out. |
| `at_most_every` | Skips the handler until this long has passed since it last ran. The handler must be a function the app defines. |

| Event | Fields | When |
|---|---|---|
| `open`, `close` | none | The app comes to the front, or leaves it. |
| `screen_on`, `screen_off` | none | The display turns on or off while the app is in front. |
| `tick` | `time` | Every `every`, lined up with the clock, at most every 20 ms. Under a minute needs `while_open: true`. |
| `time` | `time` | Once, `at` an absolute time. A signal reschedules it when it changes, and `null` turns it off. A time already passed comes at once. |
| `daily` | `time` | At a local time of day, on the weekdays in `days`. A signal holding `null` turns it off. |
| `minute`, `hour`, `day` | `time` | The local minute, hour or day changes, or the phone sets the clock. |
| `heart_rate` | `beats_per_minute`, `confidence` | A measurement every `every`. Apps asking share one measurement. |
| `steps` | `steps_today` | The step count changes, at most once a minute. |
| `step_goal` | `steps_today` | The day's step goal is reached. |
| `shake` | `kind` | A shake, found while the app is open and the screen is on. |
| `wrist_raise`, `wrist_tap`, `wrist_double_tap`, `still`, `moving` | `kind` | Accepted, but the watch does not send them yet. |
| `acceleration` | `x`, `y`, `z` in thousandths of g | Raw readings every `every`, at most 10 a second. Needs `while_open: true`, and comes only while the screen is on. |
| `battery_low`, `charging_starts`, `charging_stops`, `charging_completes` | `percent` | The battery goes low, or charging starts, stops or completes. |
| `phone_connects`, `phone_disconnects` | `is_connected` | The phone connects or disconnects. |
| `notification` | `id`, `app`, `title`, `body`, `time` | A notification arrives from the phone. |
| `call` | `kind`, `caller` | A call is `incoming`, `answered` or `ended`. |
| `music`, `weather`, `navigation` | none | The phone's player, forecast or navigation changes; read the new values from `current_music`, `current_weather` or `current_navigation`. |
| `settings_change` | `key` | A setting changes, from the watch or the phone. `key` is its name as `set_setting` takes it, or `do_not_disturb`. |
| `message` | the fields its parameter's type names | The phone sends this app a message, `{"t":"app","ch":"<app id>","d":{...}}`. |

When several of the same event are waiting, only the latest is delivered. A closed app hears only
its timers, `tick`, `time`, `daily`, the clock changes and `heart_rate`, and `message`.

#### Messages from the phone

The phone's companion app or one of its plugins can send a watch app a message on the app's own id.
The handler's parameter type names the fields the app reads, written out or as a `type`, so the
compiler knows each one's place: up to 4 numbers or booleans and 1 string. A field the phone left
out reads as 0, `false` or empty text.

```tsx
type Stop = { on: number; reason: string };

function heard(message: Stop) {
  if (message.on === 0) is_active.value = false;
}

on("message", heard);
```

The text is up to 120 bytes and, like a notification's title, cannot be kept in state; show it
with `show_toast` or `notify`, or compare it. Without `while_open: true` the handler also runs
while the app is closed: the watch loads the app without a screen, runs it and saves its state, as
for a timer, so a background run may vibrate, notify or `wake_screen`. A message that comes while
the watch lacks the memory, or while the phone holds the app's saved state, is dropped. Reading
messages needs the `phone_messages` permission, and `background` without `while_open`. A handler run in the
background may vibrate and notify; `wake_screen` brings the app to the front on the screen its `go`
named.

## Widgets

The app host draws each widget from its values. The second column of each table names the LVGL
widget it matches, where there is one. Each widget takes the common props plus its own.

### Common props

| Prop | Holds |
|---|---|
| `x`, `y` | A place in pixels inside the parent's padding, taking the widget out of the flow. |
| `width`, `height` | Pixels, or `"100%"` and other percentages; left out, the layout decides. `size` (or `diameter`) sets both. |
| `grow` | The share of spare space this widget takes in a `Row` or `Column`. |
| `padding`, `margin` | Space inside and outside the widget. Both start at 0. |
| `background`, `color`, `border_color` | `"#rrggbb"` or a theme name: `background`, `surface`, `text`, `muted`, `accent`, `success`, `warning` or `danger`. A theme name follows the user's theme. |
| `border_width`, `radius` | The border and corner rounding. |
| `font` | One of the [fonts](#fonts). |
| `align` | `start`, `center` or `end`, across the parent's flow. |
| `opacity` | 0 to 255, multiplied down through children. |
| `animate` | Accepted, but the watch does not animate yet; the value jumps. |

Containers (`Screen`, `Column`, `Row`, `Card`, `Page`, `Scroll`, `List`) also take `gap`, the
pixels between children, `justify` along the flow (`start`, `center`, `end`, `space_between`) and
`align_items` across it (those, and `stretch`). A `Button` takes `disabled`, which greys it out and
ignores touches.

Keep 8 pixels between content and the screen's edges, with `padding={8}` on the screen, and 6 to 8
pixels between controls, with `gap`. Leave the padding out only for content meant to fill the
screen, such as a game board.

### Fonts

| Font | Capital height | Use |
|---|---|---|
| `small` | 12 px | Dense text, such as a list's second line. |
| `normal` | 14 px | Body text; the default. |
| `medium` | 22 px | Headings and values. |
| `large` | 31 px | Big values. |
| `huge` | 57 px | A clock face's digits. |
| `giant` | 87 px | The biggest digits; proportional, the rest monospaced. |
| `condensed` | 35 px | Narrow tall text, such as a stopwatch's time. |
| `dot_matrix` | 7 dots | Digits, capitals, `-`, `:`, `.` and `/` drawn as dots; lower case shows as capitals. |

Every text font draws printable ASCII, the Latin-1 letters and signs, curly quotes, dashes, the
ellipsis, bullet and euro sign, and `°±×÷←✓✕`. All live in the shared library. `dot_matrix` is the
exception: an app that uses it carries it in its own image, about 1.4 KB that takes RAM while the
app is loaded, and an image built before it did shows `normal` in its place.

### Layout

| Widget | LVGL | Use |
|---|---|---|
| `Screen` | | The outermost widget of a screen: a `Column`, or a `Row` with `direction="row"`. Takes `onSwipe`, `onShow` and `onHide`, and the touch events. |
| `Column`, `Row` | container, flex | Children one after another, down or across. |
| `Grid` | container, grid | Children in rows of `columns`, such as a keypad. |
| `Stack` | container | Children on top of each other, such as text over an image. |
| `Scroll` | scroll view | Content longer than its space, scrolled by dragging and flung to glide. |
| `List` | list | A scrolling list of rows. A `.map` inside it is one repeated row, and with a fixed row `height` the watch builds only the rows on screen. |
| `Pages` | tile view | Pages side by side, one on show, swiped left and right with a wipe, and a bar along the bottom. `value` is the page on show; `onPageChange` hears each turn, and an app that turns pages itself binds `value` to a signal it also sets there. A swipe past either end goes to the screen's `onSwipe`. |
| `Page` | | One page of a `Pages`: a `Column` that fills it, or with `scrolls` a `Scroll`. `scrolls` is written out. |

`Scroll` and `List` take:

| Prop | Holds |
|---|---|
| `direction` | `"column"`, the default, to scroll down, or `"row"` to scroll across; a repeated row across a `List` needs a fixed `width` to be built only while in view. |
| `scrolling` | `"smooth"`, the default, follows the finger and glides when flung; `"pages"` settles on a whole page when the finger lifts: a drag of 20 pixels or more turns the page. |
| `scroll_bar` | `false` leaves the bar out. While the content is longer than the widget, the watch draws a 2-pixel bar down its right edge, or along the bottom when it runs across, and keeps 4 pixels for it: a thumb over a dim track, or a segment per page. |

All three are written out in the source.

### Showing things

| Widget | LVGL | Use and props |
|---|---|---|
| `Label` | label | Text. `text_align` (`left`, `center`, `right`); `long_mode`: `wrap` (the default, at spaces), `scroll` (one line, sliding back and forth) or `clip`. |
| `Icon` | image | A shared library `icon`, drawn in `color` at `size`. |
| `Image` | image | `src`, a PNG beside the source or a library icon; `recolor` draws a one-colour image in that colour; `width` and `height` for one that is not square. |
| `Line` | line | `points`, a fixed-length array of `{ x, y }` in the parent's pixels, joined in order; `thickness`. |
| `Arc` | arc | A ring or part of one from `value` between `minimum` and `maximum`; `start_angle`, `end_angle`, `thickness`, `track_color`, `diameter`. Its children are stacked in its middle. It cannot be dragged yet. |
| `Bar` | bar | A straight progress bar: `value`, `minimum`, `maximum`, `track_color`. |
| `Led` | led | A dot in `color`, with `diameter`. |
| `Chart` | chart | `values`, one array drawn in `color`, with `minimum` and `maximum`. `series` takes only its first series on the watch. |
| `Scale` | scale | Ticks around a dial: `ticks`, `major_every`, `labels` for the major ticks, `major_color`, `skip` for ticks to leave out, `start_angle` and `end_angle` for a part dial, and `value` with `needle_color` for a gauge. |
| `AnalogClock` | | Hands for `time` (normally `now.value`): `show_seconds`, `hour_color`, `minute_color`, `second_color`, `hour_length`, `minute_length`, `second_length`, and `hand_style` (`line`, `tapered`, `rounded`). |
| `CellGrid` | | A board of coloured cells from one array, such as Life, Snake or 2048, as one widget however many cells. See below. |
| `BatteryGauge` | | A battery outline filled to `value` percent, with a bolt while `charging`; `single_color`, `show_bolt`, `show_percent`. |
| `StatusRow` | | Status icons chosen by `status_flags` (1 notifications, 2 the phone, 4 an alarm), then a battery; `icon_size`, `gap`, and the battery props. |
| `Badge` | | An `icon` with text beside it as one widget; `icon_size`, `gap`, `recolor` for the icon. |
| `AppList` | | The installed apps, each its icon and name, as one widget however many. See below. |
| `TextFile` | | A text file from storage, wrapped and scrolled: `path`, `font_path` (a file naming `small`, `normal`, `medium` or `large`, which wins over `font`), `placeholder` shown in `muted` while the file is missing or empty, and `scrolling` and `scroll_bar`. Only the lines near the view are read into RAM. The file is read when its screen opens; at most 32,767 pixels of lines are shown. |
| `TextLines` | | The last lines of text kept as character codes: `codes`, an array signal, and `length`, how many are in use. Wraps at spaces and breaks at new lines. |
| `BitmapFile` | | A one-bit picture from storage, such as a QR code: `path`, and `value`, a number written after the path, so `path="/qr/"` with `value={2}` shows `/qr/2`. The file is a width and a height of at most 120, then each row packed eight pixels to a byte, leftmost in the high bit; set pixels draw in `color`, each as large a square as fits. |

`CellGrid` props: `columns`, `rows`, `cell_size` (the side of one cell; without it the cells grow to
fill the widget), `cell_gap` (pixels between cells), `cell_shape` (`square`, `rounded`, `round`),
`cells` (one number per cell, a colour from `palette`), `palette` (a constant array or an array
signal), `show_numbers` with `number_map` and `number_colors` to draw each non-zero cell's number,
and `onClick` and `onDrag` receiving `{ value }`, the cell's number.

`AppList` props: `gap` between rows, `color` for icons and names, `first` and `count` for one run of
`installed_apps`, `sections` for every app under its group's name, `columns` above 1 for square
cells that many across, `scrolling` and `scroll_bar`, and `onClick` receiving `{ value }`, the app's
place in `installed_apps`. Rows are 52 pixels tall.

### Input

| Widget | LVGL | Use and props |
|---|---|---|
| `Button` | button | A tappable button with text or an icon; `disabled`. Its `background` is `surface` and its `radius` 12 unless set. |
| `ClickArea` | | An invisible area that takes touches; its children are stacked down it. |
| `Switch` | switch | On or off; `value` a boolean or a signal. |
| `Checkbox` | checkbox | Ticked or not, with its children as the label; `value` a boolean or a signal. |
| `Slider` | slider | A value dragged along a track: `value`, `minimum`, `maximum`, `step`. |
| `Roller` | roller | A wheel of `options`; `value` is the chosen option's number. |
| `ButtonMatrix` | button matrix | A grid of buttons drawn as one widget: `rows`, an array of rows of labels, written out; `onClick` receives `{ value }`, the button counted across the rows; `highlighted` lists labels drawn in the accent colour (the first 32 buttons only). |
| `NumberRoller` | | Rollers for a number from `minimum` to `maximum`, padded to `digits`. |
| `TimeRollers` | | Hour and minute rollers for `value`, a minute of the day; `twelve_hour` adds AM and PM. |
| `DurationRollers` | | Rollers for `value` in seconds; `show_hours`. |
| `DayButtons` | | Seven day buttons over `value`, a day mask; `columns` a row. |
| `Stepper` | | Minus and plus buttons around the value or its `text`, stepping by `step` between `minimum` and `maximum`. |
| `ConfirmBox` | | A question and two buttons: `text` holds the cancel button's text, the confirm button's and the question, each on its own line; `onChange` sends 1 or 0. |

The rollers, `DayButtons`, `Stepper` and `ConfirmBox` send the new value to `onChange`; the
components below bind them to signals.

### Components

These come from `battletime/components`, built from the widgets above. The compiler inlines each into
the app.

| Component | Use |
|---|---|
| `Card` | A rounded panel in `surface`, radius 12, padding 10 and gap 4 unless set: the standard block of an app's layout. |
| `Header` | A screen's `title` row, with an optional `icon` and `color`. |
| `ScrollBar` | For paging by hand: one segment per page down the right edge for `page` of `pages`. Put it straight in a `Screen` without padding, and pad the content beside it by 4 or more; `Pages` draws its own bar. |
| `BatteryIcon` | The watch's own battery as one widget: `show_percent`, `color`, `single_color`, `show_bolt`. |
| `StatusIcons` | The status row bound to the system: notifications while any are unread, the phone while connected, the alarm with `show_alarm`, then the battery. `icon_size`, `gap`, `color`, `battery_color`, `align`. |
| `StepsBadge`, `HeartRateBadge`, `TemperatureBadge` | Today's steps, the heart rate while measured, or the temperature in the wearer's units, beside an icon. `icon_size`, `gap`, `icon_color`, `align`; `TemperatureBadge` takes the weather `icon`. |
| `ConfirmDialog` | A `question` with two buttons, shown while the `open` signal is true and closing itself on either: `confirm_text`, `cancel_text`, `onConfirm`, `onCancel`. |
| `NumberPicker`, `TimePicker`, `DurationPicker` | Rollers writing back to a signal. `TimePicker` follows the 12 or 24 hour setting. |
| `WeekdayPicker` | Day buttons toggling a signal of day bits, Monday in bit 0; `columns` a row. |

### Icons

`icons.<name>` names a shared library icon: `alarm`, `backspace`, `battery`, `bell`, `bell_off`,
`bluetooth`, `calculator`, `calendar`, `charging`, `chart`, `chat`, `check`, `clear`, `clock`,
`close`, `clouds`, `dice`, `few_clouds`, `flag`, `flashlight`, `game`, `heart`, `heavy_clouds`,
`info`, `level`, `metronome`, `minus`, `mist`, `moon`, `music`, `navigation`, `news`, `next`,
`none` (empty), `note`, `notification`, `pause`, `phone`, `phone_missed`, `pine_logo`, `play`,
`plus`, `previous`, `qr_code`, `rain`, `remote`, `settings`, `showers`, `sms`, `snow`, `steps`,
`stop`, `stopwatch`, `sun`, `thunderstorm`, `timer`, the `turn_` arrows (`arrive`, `left`, `right`,
`roundabout`, `sharp_left`, `sharp_right`, `slight_left`, `slight_right`, `straight`, `u_turn`),
`voicemail`, `volume_down`, `volume_up`, `weather`, `weather_night_clear` and
`weather_night_cloudy`.

| Function | Gives |
|---|---|
| `icon_named(name)` | The icon of that name, for one chosen by state, such as `` icon_named(`turn_${turn}`) ``. |
| `library_icon(index)` | The icon at that place in the library's order (`ICON_NAMES` in `compiler/src/types.rs`), or `none` outside it, for an icon a file chose by number. New icons go at the end. |

### Not available yet

The compiler knows these LVGL names, but the SDK offers no component for them and the watch draws
each as an empty box: `Tabs`, `RichText`, `AnimatedImage`, `ArcLabel`, `Table`, `Calendar`,
`ImageButton`, `Radio`, `Spinbox`, `Dropdown`, `TextArea` and `Keyboard`. `Spinner` is in the SDK
and the preview but draws as an empty box on the watch. These props are left out with a warning:
`brightness` on `Led`, `visible_rows` on `Roller`, `kind` on `Chart` and `rounded` on `Line`.

Left out for good: a canvas, whose pixel buffer would not fit in RAM (draw with `Line`, `Arc` and
`Image`); windows and message boxes, which `Header` and `ConfirmDialog` cover.

## Effects

Effects are calls a handler makes to ask the system to do something. They run after the handler
finishes without failing, in the order they were called.

### Haptics

| Effect | Does |
|---|---|
| `vibrate(milliseconds)` | Runs the motor once, capped at 2 seconds. |
| `vibrate_pattern(pattern)` | `tap`, `double_tap`, `notification`, `alarm`, `success` or `failure`. `alarm` repeats until `stop_vibrating()`, the app closes, or a minute passes. |
| `stop_vibrating()` | Stops any pattern this app started. |

During do not disturb the watch plays only `vibrate_pattern("alarm")`, and only while
`settings.alarms_in_do_not_disturb` is on; every other vibration, `notify`'s included, is skipped.

### Display and navigation

| Effect | Does |
|---|---|
| `set_brightness(level)` | Sets the backlight, 1 to 3, until the app closes. |
| `keep_screen_on(on)` | Stops the screen turning off by itself while this app is in front. |
| `wake_screen()` | Turns the screen on and brings this app to the front. |
| `screen_off()` | Turns the screen off now. |
| `go(screen)` | Shows another of this app's screens. From an `onSwipe` it slides in the way the swipe went; otherwise it wipes in from the right. |
| `back()` | Returns to the previous screen, or leaves the app from its first one. |
| `open_app(id)` | Opens another installed app; the id may be worked out, such as `installed_apps.value[i].id`. |
| `close()` | Leaves the app for the watch face. |
| `show_toast(text)` | Shows a short message over the screen for two seconds. |

### Notifications and phone

| Effect | Does |
|---|---|
| `notify(title, body)` | Adds a notification on the watch under the app's name. |
| `dismiss_notification(id)` | Removes a notification. |
| `mark_notifications_read()` | Marks every notification read. |
| `send_to_phone(channel, data)` | Sends the companion `{"t":"app","ch":channel,"d":data}`, such as `send_to_phone("remote", { button: 2 })`. `data` is up to 4 whole numbers; the channel and keys are written out, each 1 to 32 letters, digits, `_` or `-`. See [companion-protocol.md](companion-protocol.md#calls-music-weather-and-the-rest-served). |
| `music_control(command)` | `play`, `pause`, `next`, `previous`, `volume_up` or `volume_down`. |
| `call_control(command)` | `answer`, `reject` or `mute` the current call. |
| `find_phone(on)` | Makes the phone ring, or stops it. |
| `request_music_state()`, `request_navigation_state()` | Asks the companion for the player's state or the navigation step now; the answer arrives as the `music` or `navigation` event. Gadgetbridge ignores them. |

### Sensors

| Effect | Does |
|---|---|
| `measure_heart_rate()` | Takes one reading now; it updates `heart_rate`. |
| `start_heart_rate()`, `stop_heart_rate()` | Measures continuously while the app is open; it stops when the app closes. `stop_heart_rate()` ends only this app's own measuring. |
| `set_heart_rate_led(on, brightness, period)` | Drives the sensor's LED directly, such as for a beacon: `brightness` 0 to 3, `period` the flash period in milliseconds, 0 for steady. Turned off when the app closes. |
| `reset_step_trip()` | Starts `step_history.trip` from zero. |

### System and storage

| Effect | Does |
|---|---|
| `set_setting(key, value)` | Changes a setting by name: any key of `settings`, `brightness` (1 to 3), `do_not_disturb` or `bluetooth`. Booleans are 1 or 0, and a value outside its limits is ignored. |
| `set_do_not_disturb(on)` | Turns do not disturb on or off. |
| `set_bluetooth(on)` | Turns Bluetooth on or off; a restart turns it on again. |
| `set_clock(year, day_of_year, minute_of_day)` | Sets the clock to a local time at 00 seconds; `day_of_year` counts from 1, `minute_of_day` from 0. A day or minute that does not exist, or a year outside 1970 to 2200, is ignored. |
| `choose_watch_face()` | Opens the face chooser once the handler ends: the shown face full screen, which hears no touches, where a swipe left or right saves the next or previous face turned on in `watch_face` and slides it in, wrapping round, and the side button comes back to the app. Does nothing with no face installed. |
| `set_app_enabled(id, on)` | Turns an installed app or face on or off; does nothing when `packages` gives a refusal. |
| `confirm_firmware()` | Keeps a firmware on trial, so a restart no longer rolls it back. |
| `roll_back_firmware()` | Restarts a firmware on trial without confirming it, bringing back the one before. |
| `open_folder(path)` | Lists a folder into `current_folder` soon after the handler ends. The path may be worked out, such as `` `${current_folder.value.path}/..` ``; `..` never climbs above the top, and a path over 64 bytes lists nothing. Needs 1,600 B more heap. |
| `prefetch_file(path, offset, length)` | Starts reading up to 4,096 bytes of a file into RAM in the background, so the next `file_bytes` of that part takes them without waiting. One part is held at a time. Needs 4 KB more heap. |
| `log(text)` | Writes a line to the debug log. |

## Functions

These are plain functions, usable in views, `computed` and handlers unless marked otherwise.

### Numbers

Every number is a 64-bit whole number; there are no fractions. For decimals, keep a fixed number of
places in a whole number and show it with `format_fixed`. A bare `/` is refused: write
`Math.trunc(a / b)`, `Math.floor`, `Math.ceil` or `Math.round` to say how it rounds. `Math.min`,
`Math.max` and `Math.abs` are accepted, and so are `&`, `|`, `^`, `~`, `<<` and `>>`. Overflow,
division by zero and a shift outside 0 to 63 fail the call.

| Function | Returns |
|---|---|
| `random_int(lowest, highest)` | A random whole number between the two, both included. Handlers only. |
| `clamp(value, lowest, highest)` | `value` kept between the two. |

### Text

Template literals, such as `` `${steps} steps` ``, join text and numbers.

| Function | Returns |
|---|---|
| `format_number(number, minimum_digits?)` | The number as text, padded with zeros. |
| `format_fixed(value, places, trim?)` | `format_fixed(1250, 3)` is `1.250`; with `trim`, trailing zero decimals are left off. |
| `format_duration(duration, pattern)` | The duration in a pattern, such as `"mm:ss"`. |
| `format_time(instant, pattern)` | The local time in a pattern, such as `"dddd D MMMM"`, or a named pattern. |
| `format_temperature(celsius)` | The temperature in the user's units, with its sign. |
| `to_upper(text)` | The text in capitals. |
| `pad_start(text, width, character?)` | The text padded in front with spaces, or one literal character. |
| `text_length(text)` | The number of characters. |
| `slice_text(text, start, end?)` | Part of the text, counted in characters; negative counts from the end. |
| `join(texts, separator)` | The texts of a fixed-length array joined. |
| `text_from_codes(codes, length)` | Text from the first `length` character codes of an array signal. |

`.toUpperCase()`, `.slice()`, `.padStart()` and `.length` work on text too.
`to_upper` and `.toUpperCase()` capitalise only the letters the font draws, ASCII and Latin-1, with ß as SS;
other characters stay as they are.

### Format patterns

A pattern is a literal string, or a `? :` choice between literals, so the compiler checks it, and
the watch checks it again at install. Text inside `[brackets]` is copied as it is, and so is any
character that is not a letter.

| Time token | Shows |
|---|---|
| `YYYY` | The year, such as `2026`. |
| `MMMM`, `MMM` | The month's name, such as `October` or `Oct`. |
| `MM`, `M` | The month's number, `01` to `12` or `1` to `12`. |
| `DD`, `D` | The day of the month. |
| `dddd`, `ddd` | The weekday's name, such as `Sunday` or `Sun`. |
| `HH`, `H` | The hour on a 24 hour clock. |
| `hh`, `h` | The hour on a 12 hour clock. |
| `mm`, `m` | The minute. |
| `ss`, `s` | The second. |
| `a`, `A` | `am` or `pm`, `AM` or `PM`. |

The named patterns follow the 12 or 24 hour setting: `"clock"` is `HH:mm` or `h:mm`,
`"clock_with_seconds"` adds `:ss`, and `"date"` is `YYYY-MM-DD`.

| Duration token | Shows |
|---|---|
| `hh`, `h` | Hours. |
| `mm`, `m` | Minutes. |
| `ss`, `s` | Seconds. |
| `cc` | Hundredths of a second, two digits. |

The largest unit in a duration pattern shows the whole amount, so `mm:ss` shows 75 minutes as
`75:03`. A binding redraws as often as the smallest unit its pattern shows: every second with `ss`,
ten times a second with `cc`, and otherwise once a minute.

### Time

All in local time.

| Function | Returns |
|---|---|
| `year_of`, `month_of`, `day_of`, `hour_of`, `minute_of`, `second_of` | That part of an instant. |
| `weekday_of(instant)` | 0 to 6, Monday first. |
| `start_of_minute`, `start_of_hour`, `start_of_day` | The start of that local minute, hour or day. A view that shows only these redraws once a minute. |
| `next_time_of_day(after, { hour, minute }, days?)` | The first instant after `after` at that time, on one of `days`. |
| `day_of_year(instant)` | 1 to 366. |
| `iso_week_of(instant)` | The ISO 8601 week number. |

### Shared library

From `battletime/lib`. These run in the [shared library](app-runtime.md#the-shared-library), so an app
carries only a 5-byte call, except `sine`, `cosine` and `point_on_circle`, which work only with
arguments known when compiling.

| Function | Returns |
|---|---|
| `sine(degrees)`, `cosine(degrees)` | In thousandths. |
| `point_on_circle(center_x, center_y, radius, degrees)` | `{ x, y }`, 0 degrees at the top, clockwise. |
| `display_hour(instant)` | 0 to 23 on a 24 hour clock, else 1 to 12. |
| `minute_of_day(instant)` | 0 to 1439. |
| `is_night(instant)` | Whether it is between sunset and sunrise, or 18:00 to 06:00 until the phone sends them. |
| `steps_since(base)` | Steps since today's count was `base`, counting on after midnight's reset. |
| `temperature_in_unit(celsius)` | Whole degrees in the wearer's units. |
| `digit_of(value, place)` | One decimal digit: `place` 1 for the units, 10 for the tens. |
| `sunday_week_of(instant)` | The week counted from the year's first Sunday, as strftime's `%U`. |
| `days_in_year(year)` | 366 or 365. |
| `weather_icon_at(instant)` | The current weather's icon, with the moon at night. |
| `weather_condition_name(condition)` | Clear, Cloudy, Rain, Showers, Thunder, Snow or Mist. |
| `temperature_color(celsius)` | InfiniTime's colour for a temperature, blue when freezing to deep orange when hot. |
| `format_time_of_day(hour, minute)` | `07:30`, or `07:30 AM` on a 12 hour clock. |
| `day_mask_name(days)` | Once, Every day, Weekdays, Weekends, or a letter a day such as `M-W----`. |

### Records and arrays

State may hold records of a fixed shape and fixed-length arrays of numbers or records, such as a
list of alarms `{ hour, minute, days, enabled }`. A handler may write one element, as in
`board.value[cell] = 2`, or one field, as in `alarms.value[i].enabled = true`; writes apply when the
handler finishes, and later reads in the same handler see them.

Text in state can only be one of the app's own literal strings. Keep text made while running as an
array of character codes and show it with `text_from_codes` or `TextLines`.

Assigning a whole array follows JavaScript: the right side reads the old array throughout. A local
holds one value, so an array cannot be kept in a handler's `const` or `let`. An app's state is at
most 2 KB and 64 variables, one array at most 1 KB, and each array stores its elements in 1, 2, 4 or
8 bytes, whichever fits. One handler may change at most 16 variables and 1 KB of state.

| Function | Returns |
|---|---|
| `range(count)` | `0` to `count - 1`, with `count` known when compiling. |
| `array.map(...)` | A new array of the same length. |
| `array.includes(value)`, `array.indexOf(value)` | Whether the value is there, and where, or -1. |
| `count_where(array, test)` | How many elements pass the test. |
| `count_of(array, value)` | How many elements equal the value. |
| `nth_index_of(array, value, n)` | Where the `n`th element equal to the value is, from 0, or -1. |
| `arrays_equal(first, second)` | Whether two arrays of one length hold the same elements. |
| `line_through(grid, width, cell, value)` | The longest line of the value through the cell, across, down or diagonally. |
| `count_inversions(array)` | How many pairs are out of order, at most 64 elements. |
| `shuffled(array, first, count)` | The array with `count` elements from `first` in a random order. Handlers only. |
| `push_shift(array, value)`, `unshift_pop(array, value)` | The array moved one place with the value added at the end or the front, as for a history of laps. |
| `file_bytes(path, length, offset?)` | `length` bytes of a file from `offset`, each 0 to 255, 0 past the end or when the file is missing. `path` is written out and `length` known when compiling; `offset` may be worked out, to page through a file larger than RAM. Read when the handler runs, unless `prefetch_file` already read that part. |

The functions from `count_of` down, and `includes` and `indexOf` on an array signal of numbers, run
natively in the firmware, so they cost a few bytes where a loop costs dozens. Their arrays must be
signals of numbers or parameters given one. `push_shift`, `unshift_pop`, `shuffled` and
`file_bytes` give a whole array, so they may only be assigned to an array signal of the same length
in a handler, as in `laps.value = push_shift(laps.value, lap)`. A grid is numbered across each row
from the top left. Work only one game needs is written in the game itself, as `apps/twos` slides
its tiles and `apps/game-of-life` steps its board with `.map` over their cells.

### Control flow

`if`, `for ... of` over a fixed-length array, early `return`, `break` and `continue` are allowed.
A call runs at most 200,000 steps; a game search should stop well inside that, or split its work
across several calls.

## Permissions

The compiler works out the permissions an app needs from the effects, events and system values it
uses, and reports them; the image does not hold them yet.

| Permission | Covers |
|---|---|
| `heart_rate` | The `heart_rate` event, the heart rate effects and `set_heart_rate_led`. |
| `motion` | The motion events, such as `shake`, and `acceleration`. |
| `notifications` | Reading `notifications`, the `notification` event and the notification effects. |
| `phone` | `call_control`, `music_control`, `find_phone`, `request_music_state`, `request_navigation_state`, reading `current_call`, `current_music` and `current_navigation`, and the `call`, `music` and `navigation` events. |
| `phone_messages` | `send_to_phone` and the `message` event. |
| `internet` | Web requests, which the bytecode does not carry yet. |
| `settings` | `set_setting`, `set_do_not_disturb`, `set_bluetooth`, `set_app_enabled`, `set_clock`, `choose_watch_face`, `confirm_firmware` and `roll_back_firmware`. |
| `wake` | `wake_screen`. |
| `background` | Events other than the app's own life connected without `while_open: true`. |

Reading other system values and files, the screen, haptics and navigation need no permission.

## Not in the bytecode yet

`vibrate_sequence` and `http_request` are in the SDK, but the compiler refuses them, because they
need an array or object argument. The `http_response` event is not in the image format.
