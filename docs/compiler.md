# The app compiler

`neotime-compiler` (`compiler/`) turns an app or watch face written as Preact components with
signals into an app image, the `.ntapp` file the runtime reads. It runs on the computer only.
[app-runtime.md](app-runtime.md) describes the design and the image format, and
[app-reference.md](app-reference.md) lists every name apps use.

## Using it

```sh
nix develop -c cargo run -p neotime-compiler -- apps/stopwatch/app.tsx -o stopwatch.ntapp
nix develop -c cargo run -p neotime-compiler -- apps/stopwatch/app.tsx --check
```

- Without `-o`, the image is written next to the source with the `.ntapp` extension.
- `--check` only reports problems and writes nothing.
- Errors and warnings print as `file:line:column: message`. An error stops the image; a warning
  says what was left out. The command exits with 1 on an error.

Every image the compiler writes has passed the runtime's own install checks, `AppImage::parse`
and `verify`.

## How it works

1. **Parse** the file with [oxc](https://oxc.rs), and the shared components in
   `sdk/neotime/src/components.tsx`, which the compiler carries inside it.
2. **Load the top-level names**: imports, constants, functions, `computed` and signals. Each
   `signal`, `persisted` and `useSignal` gets a state variable.
3. **Render each screen**: call its component once at compile time, inline every component, and
   turn the JSX into a fixed widget tree.
4. **Compile** each property that changes into an expression, and each event function into a
   handler. Identical code is stored once.
5. **Choose array widths** by running steps 2 to 4 again until the values each variable may hold
   stop growing (see [State](#state)).
6. **Write** the image with the runtime's `ImageWriter`, then check it with the runtime's checks.

Anything known at compile time is worked out then: constants, tables such as `CELLS.map(...)`,
props passed as literals, `HOUR * 2`. Only what reads state, the clock, the system or an event
becomes bytecode.

## The language

### Top of the file

| Written | Becomes |
|---|---|
| `import` from `preact`, `preact/hooks`, `@preact/signals` and `neotime`, `neotime/widgets`, `neotime/components`, `neotime/icons`, `neotime/lib` | Names the compiler knows. Any other import is an error. |
| `const NAME = value` | A constant when it can be worked out now, otherwise an expression compiled where it is used. |
| `signal(start)`, `persisted("key", start)` | A state variable; `persisted` sets the persist flag and uses the key. |
| `computed(() => ...)` | Inlined wherever `.value` reads it. |
| `function` declarations and arrow constants | Inlined wherever they are called. |
| `type` aliases | Used to read type arguments such as `signal<Mode>(...)`. |
| `export default define_app({...})` | The image's id, name, kind and screens. |

### Components

- A component is called once, while compiling. Its props become names for what was passed;
  `{...props}` passes them on.
- In a component's body, `const` names an expression, a constant or a new `useSignal`;
  `if (test) return null;` hides what follows while `test` holds.
- `{condition && <Widget/>}` and `{test ? <A/> : <B/>}` set `visible`. Conditions nest through
  fragments and components, and are joined with `&&`.
- `array.map((element, index) => <Widget/>)` is unrolled once per element. The array must be a
  constant, `range(n)` or an array signal.
- `props.children` puts the children passed to a component where it is written.
- Text children, `{expressions}` and template literals are joined into the widget's `text`.
- `<Screen>` becomes a `Column`, or a `Row` with `direction="row"`, and must be the outermost
  widget of a screen. Each screen's root gets `ScreenName`; `main` is written first.
- Array props, such as a `CellGrid`'s `cells` and `palette`, a `Roller`'s `options`, a `Line`'s
  `points`, a `Chart`'s `values` and a `Scale`'s `labels` and `skip`, name a state array: the
  property is the array's variable number, never an expression. An array signal is named as it
  is. A constant array becomes a non-persisted state array of its own, keyed `~tableN`, shared by
  every prop with the same values; it counts towards the state limits. An array that changes
  while running but is not an array signal is left out with a warning.
- `points` are stored as x and y one after the other. A `ButtonMatrix`'s `rows` become its
  `options`, with a `"\n"` element between rows, as LVGL's button matrix map has it; a click
  reports the label's place counted across all rows, without the row breaks.
- Components from `neotime/components` are inlined from the SDK source, because the image format
  cannot refer to the shared library yet.

### Handlers

An event prop takes a named function, an inline arrow, or a `neotime` effect with no arguments,
such as `onClick={back}`. The event is the function's first parameter; `event.x` reads its field
by the position `runtime/src/events.rs` gives.

Inside handlers and the functions they call:

- `let` and `const` locals, up to 16 at once. A local holds one value; an array in a local is an
  error unless it is a constant.
- `if` and `else`, early `return`, `break` and `continue`.
- `for (const x of array)` over a constant array, `range(n)`, an array signal or an array of
  records.
- Writes to `signal.value`, `array.value[index]` and `record.value.field`, with `=`, `+=` and the
  other compound operators except `/=`, and `++` and `--`.
- Effect calls from `neotime`, and `random_int`.

A whole array assigned at once follows JavaScript: the new value is worked out from the old array
before anything is written.

- A constant array is written with `FillArray`, or element by element.
- Another array signal is copied with `CopyArray`.
- An array literal of up to 12 elements pushes every element first, then writes them.
- `array.map(...)`, and longer literals, are built in a scratch array the compiler adds, then copied
  in. A scratch array is not persisted and counts towards the state limits. Arrays with the same
  length, type and element width share one, unless one is still being filled.
- A function whose body ends in `return <array>` is inlined into the assignment.

Functions are inlined, never called. A call that would recurse is an error. Before inlining a
function with statements in the middle of an expression, the values already on the stack are moved
to locals, because a loop needs an empty stack.

### Expressions

- `+`, `-`, `*`, `%`, `&`, `|`, `^`, `~`, `<<`, `>>`, the comparisons, `!`, `&&`, `||`, `??` and
  `? :`.
- A bare `/` is an error. `Math.trunc(a / b)` divides rounding toward zero, `Math.floor(a / b)`
  down, `Math.ceil(a / b)` up and `Math.round(a / b)` half up, as JavaScript would round the
  fraction. `Math.min`, `Math.max` and `Math.abs` are accepted too.
- `x === null` and `x !== null` test for the empty optional.
- `format_number`, `format_duration`, `format_time` and `format_temperature`. A pattern must be
  literal text, or a choice between literals with `? :`; it is checked when compiling.
- `hour_of`, `minute_of`, `second_of`, `day_of`, `month_of`, `year_of`, `weekday_of` and `clamp`.
- `start_of_minute`, `start_of_hour` and `start_of_day`, each one `StartOf` instruction.
- `next_time_of_day(after, { hour, minute }, days?)`, a loop over the next eight local days with
  `StartOf` and `TimePart`.
- `text_from_codes(codes, length)` with an array signal of character codes, as `TextFromCodes`.
- `join(texts, separator)` over a constant array, an array literal or an array signal, unrolled
  into `Join`s, with a literal separator.
- `to_upper`, `text_length`, `slice_text` and `pad_start`, and the methods `toUpperCase`, `slice`
  and `padStart`, on text known when compiling. On text made while running they need runtime
  instructions; see [Not supported yet](#not-supported-yet).
- `icon_named(name)` with a literal, a choice such as a weather condition, fixed text, or a
  template such as `` `turn_${turn.value}` ``.
- A constant array or object indexed at run time: numbers that go up evenly are computed, other
  tables of up to 64 entries are looked up one entry at a time, and an object can be indexed by a
  choice such as a swipe direction.
- System values with `.value`: `now`, `battery_percent`, `settings.uses_24_hour_clock` and the rest
  in `SystemValue`. `current_weather`, `current_music` and `current_navigation` are read as
  `current_weather.value.temperature` or `current_weather.temperature.value`. Their text fields,
  `location`, `artist`, `track` and `instruction`, are text made while running.
- `Array.from({ length: n }, (_, index) => ...)` with `n` known when compiling, like
  `range(n).map(...)`.
- A bare `/` between constants whose result is whole, such as `(240 - 40) / 2`.

## Type rules

The bytecode has no type tags, so the compiler gives every value one of these types:

| Type | Comes from | Stored as |
|---|---|---|
| number | numeric literals, `number`, `Instant`, `Duration` | a whole number |
| boolean | `true`, `false`, comparisons | 0 or 1 |
| fixed text | string literals, `string` state | a string of the image |
| text | template literals, `format_*` | a string, or a slice of the call's text buffer |
| colour | `"#rrggbb"`, theme names, `Color` | RGB565, or 65536 plus the theme colour's number |
| icon | `icons.name`, `icon_named` | the icon's number |
| choice | a fixed set of names, such as fonts and swipe directions | the name's position in the set |

- Any of them may also be `null`, the empty optional.
- A string literal takes the type the place it goes to expects: `color="accent"` is a colour,
  `font="large"` a font, `event.direction === "left"` a swipe direction.
- The two branches of `? :`, and both sides of `&&`, `||` and `??`, must have the same type.
- Template literals show numbers in decimal and choices by name.
- Only fixed text can be compared or kept in state. Text made while running lives in the call's
  text buffer, which the next call reuses.
- A value that may be null is accepted where it may not be. The compiler does not follow tests
  such as `if (x.value !== null)`, so it relies on TypeScript to have checked them.
- Numbers must be whole; a fraction is an error.

## State

- Each `signal`, `persisted` and `useSignal` is one state variable. A record, such as
  `signal({ hour: 8, minute: 0 })`, is one variable per field, keyed `name.field`.
- An array of records, such as `signal([{ hour: 7, enabled: false }, ...])`, is one array per
  field, also keyed `name.field`. `alarms.value[i].enabled = true` is one element write, and
  `alarms.value[i] = { ...alarms.value[i], hour: 9 }` works out every field before writing any.
  Such an array can be looped over and mapped into widgets, but not assigned whole.
- An array whose elements start with different values, such as `signal([3, 1, 2, 0])`, is written
  with each element's starting value (section 8, `ImageWriter::array_with_values`). An array of
  records may start with different records the same way.
- An array stores each element in 1, 2, 4 or 8 bytes. The compiler finds the values each variable
  may hold: its start, every value written to it, the values of arrays copied into it, and what
  each operation can give, such as `x % 10` or a `? :` between two literals. It compiles once
  assuming only the starting values, then again with what it found, until nothing new turns up,
  at most six times; after that every array is 8 bytes. `Instant` and `Duration` arrays are always
  8 bytes.
- The limits are the runtime's: at most 64 variables, 2 KB of state and 1 KB for one variable, and
  128 widgets.

## Number mappings

These numbers are stored in widget properties and effect arguments. The runtime loop must read
them the same way.

| Property or argument | Numbers |
|---|---|
| `font` | 0 small, 1 normal, 2 medium, 3 large, 4 huge, 5 giant, 6 seven_segment, 7 dot_matrix, 8 condensed |
| `align` | 0 start, 1 center, 2 end |
| `long_mode` | 0 wrap, 1 scroll, 2 clip |
| `x`, `y`, `width`, `height` | pixels; a negative number is a percentage of the parent, so `"100%"` is -100 |
| `grow` | a weight; `padding`, `margin`, `border_width` and `radius` are pixels |
| `visible`, `show_seconds` | 0 or 1 |
| `start_angle`, `end_angle`, and every other angle | degrees clockwise from twelve o'clock, passed through unchanged; the host converts them for rlvgl |
| `icon`, `src` | the icon's position in `ICON_NAMES` in `compiler/src/types.rs`: the first 58 in the order of `sdk/neotime/src/icons.ts` up to `weather` (57), then `none` 58, `weather_night_clear` 59, `weather_night_cloudy` 60 and `pine_logo` 61; new icons go at the end so numbers never change |
| `text`, screen names, `go`, `open_app`, `set_setting`'s key | a string of the image |
| `opacity` | 0 to 255 |
| `disabled` | 0 or 1 |
| `gap`, `cell_size`, `cell_gap`, `hour_length`, `minute_length`, `second_length` | pixels |
| `justify`, `align_items` | 0 start, 1 center, 2 end, 3 space_between (`"between"` too) |
| `columns`, `rows` (CellGrid), `ticks`, `major_every` | counts |
| `cell_shape` | 0 square, 1 rounded, 2 round |
| `hand_style` | 0 line, 1 tapered, 2 rounded |
| `needle_color`, `hour_color`, `minute_color`, `second_color`, `recolor` | colours, as above |
| `cells`, `palette`, `options`, `points`, `values` (Series), `labels`, `skip` | the number of the state array holding them |
| `vibrate_pattern` | 0 tap, 1 double_tap, 2 notification, 3 alarm, 4 success, 5 failure |
| `music_control` | 0 play, 1 pause, 2 next, 3 previous, 4 volume_up, 5 volume_down |
| `call_control` | 0 answer, 1 reject, 2 mute |
| `find_phone()` | 1 when no argument is given |
| `request_navigation_state()` | the `RequestNavigationState` effect, with no arguments |
| `current_weather.condition`, `WeatherCondition` | 0 clear, 1 few_clouds, 2 clouds, 3 heavy_clouds, 4 rain, 5 showers, 6 thunderstorm, 7 snow, 8 mist |
| `NavigationTurn` | 0 straight, 1 left, 2 right, 3 slight_left, 4 slight_right, 5 sharp_left, 6 sharp_right, 7 u_turn, 8 roundabout, 9 arrive |

Swipe directions, button presses and every event's fields, such as the `value` a click on a
`ButtonMatrix` or `CellGrid` carries, follow `runtime/src/events.rs`.

Effect arguments that are text may point into the call's text buffer, such as the text of
`show_toast(\`${count} left\`)`, so the runtime must copy them before it runs the next call.

## Not supported yet

Left out with a warning, because the image format has no place for them yet:

- `subscriptions`, the app's `icon` and other resources, `permissions`, and the `onOpen`,
  `onClose`, `onScreenOff` and `onScreenOn` lifecycle handlers.
- Props with no `Property`, such as `size`, `diameter`, `thickness`, `track_color`, `text_align`,
  `major_color`, `highlighted`, `kind` on a `Chart` and `series`, and props a widget does not have
  in `Property::belongs_to`.
- `justify="around"` and `align_items="stretch"`, which the runtime has no number for.

Errors, until the compiler or the runtime grows them:

- Assigning a whole array of records, objects other than records in state, and arrays in locals.
- `vibrate_sequence`, `http_request` and `send_to_phone`, and `sine`, `cosine`
  and `point_on_circle` unless their arguments are known when compiling.
- The text functions on text made while running, each of which needs a runtime instruction:
  `to_upper` a `ToUpper`, `text_length` a `TextLength`, `slice_text` a `SliceText`, and `pad_start`
  a `TextLength` or a `PadStart`.
- More than 16 loops, or more than 16 branches waiting to join, in one handler or expression,
  counting the functions it inlines; the runtime's checks allow no more.
- A screen component that returns different roots, such as an early `return <Screen>`.
- Fonts by name beyond the built-in set, and the app's own images.
