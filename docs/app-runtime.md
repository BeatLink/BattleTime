# App runtime

This page proposes how NeoTime apps are written, stored and run. Nothing here is built yet, and
the details will change as the runtime is written and measured. [app-reference.md](app-reference.md)
lists the events, widgets and effects apps can use.

## Goals

- **Apps are data, not programs.** An app declares its name, icon, state, screens, the events it
  answers and the sensors it wants. The runtime owns everything that runs: it schedules work,
  calls the app's functions, persists state and draws the screen with rlvgl.
- **Functions are pure.** An app function takes the current state and an event, and returns the
  state changes and the effects it wants, such as a vibration. It cannot touch hardware, loop
  forever or keep anything between calls.
- **Event driven, not ticking.** The runtime works out from what a screen shows when it must
  redraw, and sleeps otherwise. A stopwatch stores its start time and shows `now - start`, so
  nothing ticks while it runs.
- **Apps live on the external flash.** An app is read from its file a section at a time. Only
  its state, its expressions and the open screen take RAM, inside the 12 KB the
  [memory map](memory-map.md) gives the open app.
- **What apps share lives in the firmware's flash.** The internal flash holds no apps. Beside the
  firmware and its runtime, it keeps a shared library of images, fonts, components and functions
  that every app can use, so each app stays small.
- **A broken app cannot break the watch.** Every app image is checked when it is installed, and
  every call ends: a call that loops is stopped after 200,000 steps.

## Choosing the format

| Option | How it would work | Why not |
|---|---|---|
| A JavaScript engine, such as [MicroQuickJS](https://github.com/bellard/mquickjs) | Apps ship as JS bytecode and run in the engine. | The engine alone needs about 10 KB of RAM and 100 KB of flash, the whole app budget before any state. |
| [Preact](https://preactjs.com) running on that engine | Apps are components that re-render on every change. | It needs the engine plus a virtual DOM, and re-renders on every change, which is the ticking this design avoids. Apps are still written as Preact components, but compiled ahead of time (see below). |
| WebAssembly, such as wasmi | Apps compile from any language to Wasm. | A Wasm memory page is 64 KB, all of the chip's RAM. The custom page sizes proposal would fix that, but runtime support is uncertain, and sandboxed native code is more than pure functions need. |
| [Slint](https://slint.dev) | Its language has a widget tree with property bindings and callbacks. | Its runtime targets chips with around 264 KB of RAM, and it is licensed GPL or commercially. Its language is still a good model to borrow from. |
| **NeoTime's own bytecode** | A compiler on the computer turns each app into a compact image that the runtime reads from flash. | This is the proposal. It costs a compiler and a small interpreter, both written in Rust. |

The closest existing designs evaluate the screen's bindings in the renderer, not in app code:

- [Wear OS dynamic expressions](https://developer.android.com/training/wearables/data/dynamic) bind
  a tile's text to an expression such as "platform time minus start", and the system keeps it
  current.
- [Remote Compose](https://proandroiddev.com/remotecompose-another-paradigm-for-server-driven-ui-in-jetpack-compose-92186619ba8f)
  ships a UI as a binary document, with a small RPN expression engine that has time and sensors
  built in.
- [Fitbit apps](https://dev.fitbit.com/build/guides/application/) pair a markup layout with
  handler code and load one view at a time.
- [Pebble's Alloy](https://moddable.com/blog/pebble/) runs standard JavaScript on Moddable's XS
  engine, with Piu as its declarative UI, on watches with far more RAM.

## How it fits together

```mermaid
flowchart LR
    source["app.tsx<br/>Preact components and signals"]
    preview["browser preview<br/>real Preact"]
    compiler["neotime-app-compiler<br/>checks and compiles"]
    image["app image<br/>(.ntapp)"]
    install["install over BLE<br/>verified on the watch"]
    store["/apps on external flash,<br/>cached in internal flash"]
    runtime["app runtime<br/>runs functions, draws screens"]

    source --> compiler --> image --> install --> store --> runtime
    source --> preview
```

## Writing an app

Apps are written as [Preact](https://preactjs.com) components in TypeScript, with state held in
[Preact Signals](https://preactjs.com/guide/v10/signals/). Preact never runs on the watch. The
compiler reads the components ahead of time and produces the app image, while the same source runs
on real Preact in a browser for previews.

Signals were chosen because they match what the runtime does. A `signal` holds a value, and a
`computed` is a value worked out from other signals that knows which ones it reads. On the watch, a
signal becomes a state slot, and a `computed` or a piece of JSX that reads signals becomes an
expression with an input mask. Only expressions whose inputs change are evaluated again, and
nothing re-renders.

```tsx
import { computed } from "@preact/signals";
import { define_app, persisted, now, format_duration } from "neotime";
import { Column, Label, Button } from "neotime/widgets";

const started_at = persisted<Instant | null>("started_at", null);
const stopped_after = persisted<Duration>("stopped_after", 0);

const elapsed = computed(() =>
  started_at.value ? now.value - started_at.value : stopped_after.value,
);

function toggle() {
  if (started_at.value) {
    stopped_after.value = now.value - started_at.value;
    started_at.value = null;
  } else {
    started_at.value = now.value - stopped_after.value;
  }
}

function reset() {
  stopped_after.value = 0;
}

function Stopwatch() {
  return (
    <Column>
      <Label font="large">{format_duration(elapsed.value, "mm:ss")}</Label>
      <Button onClick={toggle}>{started_at.value ? "Stop" : "Start"}</Button>
      {!started_at.value && <Button onClick={reset}>Reset</Button>}
    </Column>
  );
}

export default define_app({
  id: "stopwatch",
  name: "Stopwatch",
  icon: "./stopwatch.png",
  screens: { main: Stopwatch },
});
```

`Stopwatch` is never called on the watch. The compiler turns it into a fixed widget tree. The
label's text becomes one expression that reads `started_at`, `stopped_after` and `now`, and because
its pattern shows whole seconds the runtime redraws it once a second while the screen is on and the
stopwatch runs. The Reset button's `{... && ...}` becomes a binding on its `visible` property.

### From Preact to the watch

| In the source | On the watch |
|---|---|
| `signal(...)` at the top of the file | A state slot that lasts while the app is open. |
| `persisted("key", ...)` | A state slot saved to the app's key-value store, so it survives a reboot. |
| `useSignal(...)` inside a component | A state slot for that place in the tree, cleared when the screen closes. |
| `const` values that never change | Constants, kept in flash. |
| `computed(...)` | An expression with its own input mask. Expressions that read it inherit its inputs, and it is evaluated once however many widgets show it. |
| A function component and its props | Inlined into the widget tree wherever it is used. |
| JSX that reads `.value` | Expressions bound to widget properties. |
| `{condition && <Widget/>}` | The widget's `visible` property. |
| `array.value.map(...)` over a fixed-length array | A repeated row, one copy per element. |
| `onClick`, `onSwipe`, `onButton` | Handlers. An arrow function written inline is lifted out into a handler of its own. |
| `now`, `heart_rate`, `steps`, `battery` from `neotime` | System values the runtime provides. |
| `neotime/components`, `neotime/lib`, `icons`, fonts | References by number into the [shared library](#the-shared-library), never copied into the app. |

### Handlers

A handler is a function that writes to signals. It looks like mutation, but the compiler treats the
whole function as one transaction, as Signals' own `batch` does. Reads after a write see the new
value, and the writes are applied together when the function returns. A handler that fails, for
example by dividing by zero, changes nothing.

Effects, such as `vibrate(200)`, `go("settings")` and `notify("Timer done")`, are written as calls
from `neotime`. The runtime collects them and carries them out only after the handler has finished
without failing.

### Background work and screens

Anything that would be a timer or a `useEffect` in a web app is declared in `define_app` instead,
so the runtime can schedule it and run it while the app is closed:

```tsx
export default define_app({
  id: "heart-log",
  name: "Heart log",
  icon: "./heart.png",
  screens: { main: History, settings: Settings },
  subscriptions: [
    every({ minutes: 10 }, record_heart_rate),
    at(next_alarm, ring),
  ],
});
```

| Field | Holds |
|---|---|
| `id`, `name`, `icon` | Who the app is. |
| `kind` | `app`, the default, or `face`; a watch face has one screen and no buttons. |
| `screens` | One component per screen. `main` opens first, and the `go(...)` effect moves between them. |
| `subscriptions` | `every(period, handler)`, `sensor("heart_rate", handler)` and `at(instant, handler)`. An `at` time may be a signal, so changing an alarm reschedules it. |

### What the compiler accepts

Inside components and `computed`: `const`, arithmetic, comparisons, `&&`, `||`, `!`, `? :`,
calls to `neotime` functions and to the app's own functions (which it inlines), JSX, and `.map`
over fixed-length arrays.

Inside handlers, also: `let` locals, `if` and `else`, `for ... of` over fixed-length arrays,
writes to `.value`, effect calls and `random_int`. Numbers are whole numbers, and the
[app reference](app-reference.md#numbers) lists the `Math` functions accepted.

It rejects anything whose running time or state the runtime cannot see, and the error message
names the declarative form to use instead:

| Rejected | Use instead |
|---|---|
| `useState`, `useReducer`, `useRef` | `signal` or `useSignal` |
| `useMemo` | `computed` |
| `useEffect`, `setTimeout`, `setInterval` | `subscriptions`, or a binding that reads `now` |
| `while`, recursion | `for ... of` over a fixed-length array |
| `async`, `fetch`, classes | Not available; data from the phone arrives through a subscription |
| Imports other than `preact`, `@preact/signals` and `neotime` | Copy the code into the app |
| Creating a signal inside a handler | Declare it at the top of the file |

### Previewing in a browser

The `neotime` package also has a browser version. It draws each widget as a Preact component
inside a 240x240 frame styled like rlvgl, keeps `persisted` values in the browser's storage,
updates `now` from the computer's clock, and has a panel to fire sensor readings and subscriptions
by hand. An app can therefore be developed with Vite's hot reload and the Preact devtools.

The preview runs real JavaScript, so it would accept code the watch cannot run. The preview's dev
server therefore runs the NeoTime compiler on every save and shows its errors over the page.
The preview is close but not exact; the [simulator](simulator.md) shows the real pixels.

### Other ways to write apps

The image format does not depend on the source language, so another front end can be added later:

- **NeoTime's own language,** shaped like Slint or QML. It would read more cleanly and set no
  false expectations, but it needs its own parser, error messages and editor support.
- **KDL or RON for the tree, with expressions in strings.** This is the quickest way to feed the
  runtime while the bytecode settles, but it is unpleasant for logic.

## The compiler

`neotime-app-compiler` is a `std` command-line tool in the workspace, never built for the watch.

1. Parse the source with [oxc](https://oxc.rs), a TypeScript and JSX parser written in Rust.
2. Check that it stays inside the accepted subset, and resolve names against the `neotime` types.
3. Infer the type of every value. The bytecode carries no type tags, so types must be known here.
4. Give every signal, `persisted` and `useSignal` a state slot.
5. Inline the components of each screen into a widget tree, and turn every `computed` and every
   property that reads a signal into an expression.
6. Lift inline event functions out into handlers, and compile expressions and handlers to bytecode.
7. Work out each expression's inputs and redraw period.
8. Convert the icon and images to RGB565 with run-length encoding.
9. Write the image, and run the same verifier the watch uses.

Editors get type errors from TypeScript itself, through the type declarations in the `neotime`
package. The compiler does not depend on it.

## The app image

The `neotime-runtime` crate (`runtime/`) reads and checks images; see [Built so far](#built-so-far).
An image is little-endian. It starts with a 20-byte header and a table of sections, and the runtime
reads the sections it needs in place, never copying a whole image into RAM.

| Bytes | Holds |
|---|---|
| 0–3 | Magic `NTAP`. |
| 4–5 | Format version, now 1. |
| 6 | Kind: 0 for an app, 1 for a watch face. |
| 7 | Reserved, 0. |
| 8–9 | The shared library version the app was built against. |
| 10–11 | How many sections follow. |
| 12–15 | CRC-32 (IEEE) of every byte from 16 to the end. |
| 16–17 | The string holding the app's id. |
| 18–19 | The string holding the app's name. |
| 20 on | One 12-byte entry per section: kind (2 bytes), entry count (2), offset from the start of the image (4), length (4). |

Each kind of section appears at most once:

| Kind | Section | Layout |
|---|---|---|
| 1 | Strings | A table of (offset, length) pairs, 2 bytes each, then the UTF-8 text they point into. |
| 2 | State | 16 bytes per variable: key string (2), type (1), flags (1), array length (2, 0 for a single value), reserved (2), default value (8). Flags bit 0 means the variable persists; bits 1–2 give an array's element width: 0 for 8 bytes, 1 for 1, 2 for 2, 3 for 4. An array starts with every element at the default. At most 64 variables, 2 KB in all and 1 KB for any one. |
| 3 | Expressions | A table of (offset, length) pairs, then the bytecode they point into. |
| 4 | Handlers | The same layout as expressions. |
| 5 | Widgets | A table of (offset, length) pairs, then one record per widget, described below. At most 128 widgets. |
| 6 | Subscriptions | Each period, sensor or time the app wants, and the handler to call. Not laid out yet. |
| 7 | Resources | The app's own icon and images, RGB565 with run-length encoding. Not laid out yet. |

Widgets are stored depth first, so a widget's children follow it, each with everything inside
it. Each root is one screen, and the first is the screen the app opens on. A widget record is:

| Bytes | Holds |
|---|---|
| 0 | The kind, numbered in the order of `WidgetKind` in `runtime/src/widgets.rs`. |
| 1 | How many children it has. Only containers and buttons may have any. |
| 2 | How many properties follow. |
| 3 | Reserved, 0. |
| 4 on | 6 bytes per property: its id (1), where its value comes from (1: 0 a constant, 1 an expression, 2 a handler), and the value (4): the constant itself, a string number for text, or the expression or handler number. |

Expressions do not record what they read or how often they redraw. The runtime works both out from
the code when the app opens, so a wrong compiler can never make a widget miss an update.

### Where images live

Installed images are files in `/apps` on the external flash, each with its key-value store beside
it. The external flash cannot be read in place, so the runtime reads an image a section at a time.
When an app opens, the runtime copies its expressions into RAM, because they run on every redraw.
Handlers are read from the file when they run, which happens only on an event.

## The shared library

The internal flash keeps no apps. Its last 92 KB, after the firmware, holds the shared library:
everything common to apps that would otherwise be copied into each one. It can be read in place, so
using it costs no RAM.

| Part | Holds |
|---|---|
| Images | System icons, such as the battery, Bluetooth, heart and steps, and the icons of the built-in apps. |
| Fonts | The UI fonts at each size, so apps never carry their own. |
| Components | Ready-made widget trees with their bindings, such as a card, a list row, a header, a number picker and a confirm dialog. |
| Functions | Bytecode for common work: date and time formatting, unit conversion, small maths such as trigonometry from tables, sorting a fixed-length array. |
| Notification screens | The layouts that show a notification and the list of them. The notifications themselves are on the external flash. |

In the source, apps import these from the `neotime` package, for example
`import { Card } from "neotime/components"` or `icons.heart`. The compiler refers to them by number
instead of copying them into the image, and records the library version in the image's header. The
watch refuses an app built against a library it does not have.

The library changes only when it is updated, never in everyday use. Anything that changes often,
such as notifications, settings and app state, is stored on the external flash, whose file system
spreads writes across the chip. The internal flash wears out after about 10,000 erases.

The library is checked by the same verifier as an app image. It can be updated together with the
firmware or on its own, and a new version keeps the numbers of everything the old one had, so
installed apps keep working.

## The bytecode

The interpreter is a stack machine with room for 32 values. Values are 64 bits and carry no type
tag, because the compiler has already checked every type.

| Type | Stored as |
|---|---|
| `Int` | a 64-bit signed integer; there are no fractions |
| `Bool` | 0 or 1 |
| `Instant` | milliseconds since 1970 in UTC; an empty optional, such as a stopwatch that has not started, is the smallest 64-bit number |
| `Duration` | milliseconds |
| `Color` | RGB565 |
| `Text` | the number of a string in the image, or a slice of the call's 256-byte text buffer |

Each instruction is one opcode byte followed by its operands:

| Opcode | Instruction | Operands | Does |
|---|---|---|---|
| `01` | push small | 1 byte, signed | Pushes a number from -128 to 127. |
| `02` | push int | 4 bytes, signed | Pushes a 32-bit number. |
| `03` | push long | 8 bytes, signed | Pushes a 64-bit number, such as an instant. |
| `04` | push none | | Pushes the empty optional. |
| `05` | push text | string, 2 bytes | Pushes a string from the image. |
| `06` | read state | slot, 1 byte | Pushes a state variable. In a handler, it sees the handler's own earlier writes. |
| `07` | read system | value, 1 byte | Pushes a system value (see below). |
| `08` | read event | field, 1 byte | Pushes one of the event's 8 fields. Handlers only. |
| `09`, `0A`, `0B` | duplicate, drop, swap | | Stack housekeeping. |
| `0C` | read local | local, 1 byte | Pushes one of the call's 16 locals, which all start at 0. |
| `0D` | write local | local, 1 byte | Takes a value into a local. |
| `10`–`14` | add, subtract, multiply, divide, remainder | | Whole-number arithmetic. Overflow and division by zero fail the call; division rounds toward zero. |
| `15`–`18` | negate, minimum, maximum, absolute | | |
| `1A`–`1D` | bit and, bit or, bit xor, bit not | | For masks, such as the days an alarm rings. |
| `1E`, `1F` | shift left, shift right | | By 0 to 63 places; any other amount fails the call. Shifting right keeps the sign. |
| `19` | random int | | Takes a lowest and a highest number and pushes a random one between them, both included. Handlers only, so expressions always give the same answer. |
| `20`–`25` | equal, not equal, less, less or equal, greater, greater or equal | | Push 1 or 0. |
| `26` | not | | 1 for 0, else 0. |
| `27` | is none | | 1 if the value is the empty optional. |
| `28` | jump | distance, 2 bytes | Skips forward that many bytes. There is no way to jump backward. |
| `29` | jump if false | distance, 2 bytes | Takes a value and skips forward if it is 0. `? :`, `&&` and `\|\|` compile to this. |
| `2A` | loop start | | Marks where a loop begins. The stack must be empty here. |
| `2B` | loop back | distance, 2 bytes | Goes back that many bytes, counted from the end of this instruction, to a loop start. The stack must be empty here too. |
| `30` | time part | part, 1 byte | Takes an instant and pushes its local year, month, day, hour, minute, second or weekday (0 to 6, Monday first). |
| `40` | format number | minimum digits, 1 byte | Takes a number and pushes it as text, padded with zeros. |
| `41` | format duration | pattern string, 2 bytes | Takes a duration and pushes it as text in the pattern. |
| `42` | format time | pattern string, 2 bytes | Takes an instant and pushes the local time as text in the pattern. |
| `43` | join | | Takes two texts and pushes them joined. |
| `50` | write state | slot, 1 byte | Takes a value for a state variable. Handlers only. |
| `51` | effect | effect, 1 byte | Takes the effect's arguments and records it. Handlers only. |
| `60` | read element | array, 1 byte | Takes an index and pushes that element. An index outside the array fails the call. |
| `61` | write element | array, 1 byte | Takes an index and a value and writes that element. A value too wide for the element fails the call. Handlers only. |
| `62` | copy array | destination and source arrays, 1 byte each | Copies every element; the two must be the same length. Handlers only. |
| `63` | fill array | array, 1 byte | Takes a value and writes it to every element. Handlers only. |

System values, in the order `read system` numbers them: `now`, `utc_offset`, `battery_percent`,
`is_charging`, `steps_today`, `heart_rate`, `heart_rate_measured_at`, `is_phone_connected`,
`unread_notifications`, `is_do_not_disturb`, `brightness`, `uses_24_hour_clock`,
`uses_metric_units`, `step_goal`, `is_heart_rate_running`, `is_bluetooth_enabled`, `next_alarm`,
`heart_rate_status`, and the weather's `temperature`, `condition`, `low`, `high` and
`received_at`.

Effects, in the order `effect` numbers them, with how many arguments each takes: `vibrate` (1),
`vibrate_pattern` (1), `stop_vibrating`, `set_brightness` (1), `keep_screen_on` (1),
`wake_screen`, `screen_off`, `go` (1), `back`, `open_app` (1), `close`, `show_toast` (1),
`notify` (2), `dismiss_notification` (1), `mark_notifications_read`, `send_to_phone` (2),
`music_control` (1), `call_control` (1), `find_phone` (1), `measure_heart_rate`,
`start_heart_rate`, `stop_heart_rate`, `reset_steps`, `set_setting` (2), `set_do_not_disturb` (1),
`log` (1), `request_music_state`, `set_heart_rate_led` (3), `set_bluetooth` (1).
`vibrate_sequence` and `http_request` need arrays and more arguments, and come later.

Only `loop back` goes backward. A call stops with a failure after 200,000 instructions, which
should be roughly a fifth of a second on the watch; that figure is not yet measured.

A handler never writes to the app's state directly. The first time it writes to a variable, the
interpreter copies the whole variable into a 1 KB scratch area, and every later read and write of
that variable uses the copy. When the handler finishes, the copies are written back and the
outcome lists which variables changed; when it fails, the copies are dropped. So one handler can
change at most 16 variables and 1 KB of state.

### Checking an image

The watch checks every image when it is installed and refuses one that fails. Reading the header
checks:

- the magic, the format version and the CRC;
- that every section and every table entry lies inside the image, and no section repeats;
- that every string is UTF-8, and every state variable has a known type and a default that fits it.

Then one pass over each expression and handler checks:

- that every instruction and operand is known, and every state slot and string exists;
- that every format pattern is valid, so a typo in one is caught at install;
- that every jump lands on the start of an instruction or the end of the code;
- that single-value instructions only name single values and array instructions only arrays, that
  copied arrays have the same length, and that locals are within the 16;
- that every loop back lands on a loop start, and the stack is empty at both;
- that the stack never goes below empty or above 32 values, and that every path into a jump
  target arrives with the same depth;
- that an expression ends with exactly one value and a handler with none;
- that expressions do not read events, write state, cause effects or use random numbers.

Last, a pass over the widgets checks:

- that they form whole trees no deeper than 16 levels, and only containers and buttons have
  children;
- that every property exists on its widget, appears once, and screen-wide ones such as `onSwipe`
  are only on a screen's root;
- that every event names a handler, every other property is a constant or an expression, and every
  expression, handler and string named exists;
- that constants fit: a flag is 0 or 1, a colour fits RGB565, and text names a string.

The interpreter still checks every step as it runs, so even an image that slipped past makes a call
fail rather than crashing the watch.

## The runtime

The runtime is part of the firmware, not a separate image. It runs in place from the internal flash
with the instruction cache on, as the rest of the firmware does, and the [memory map](memory-map.md)
sets aside 48 KB of the firmware's 384 KB for it. Only app data is ever read from the external
flash; no runtime code is.

### Opening an app

1. Open the image from `/apps` and read its header, checking the shared library version.
2. Load the state: defaults, overlaid with the values saved in the key-value store.
3. Copy the expressions into RAM, create the rlvgl widgets for the first screen and evaluate every
   expression once.
4. Work out from the code which inputs each expression reads, and how often the clock changes
   what it shows.

### Events

Touches, buttons and swipes reach the handlers the widgets name. A handler runs to the end, its
changes are written back to the state, and it returns which variables changed and its effects.
The runtime marks every expression whose input mask includes a changed variable as dirty, and
redraws only those properties. Persistent
slots that changed are written to the key-value store a moment later, so a burst of taps costs one
flash write.

### Time

The runtime never ticks on an app's behalf. It knows from each expression's redraw period, such as
"every second" for `mm:ss` or "every minute" for `HH:MM`, when the screen next changes. The period
is the smallest unit the expression's format patterns show, such as a second for `ss` and a minute
for anything coarser; hundredths are capped at a tenth of a second,
and an expression that reads the clock without formatting it redraws once a second. Steps are
counted on the wall clock, so a minute display changes as each minute begins. It sets one
timer for the earliest of those times, and none at all when the screen is off or nothing on it
depends on the clock.

### Background work

A subscription runs its handler even when the app is closed. For that the runtime loads only the
app's state and that one handler, never its widgets. Subscriptions from every app are merged, so
two apps that both want the heart rate every ten minutes share one measurement. Alarms are `at`
subscriptions with an absolute time, so a reboot or a clock change cannot lose them.

### Failures

A handler that overflows, divides by zero or runs past its step limit is stopped. Its changes and
effects are dropped, and the runtime shows a crash screen naming the app and the handler. A
subscription that keeps failing is turned off until the app is opened again.

## Memory

The working memory below is measured from `neotime-runtime`; the rest is still a budget.

| Part | Size |
|---|---|
| Interpreter: a stack of 32 values, 16 locals, the 256-byte text buffer and the 1 KB scratch area | 1,792 B, measured |
| A handler's results: which variables changed and up to 8 effects | 272 B, measured, only while a handler runs |
| Bindings: what each of up to 64 expressions reads and its redraw period | 1,032 B, measured |
| State: up to 2 KB of variables and arrays, and where each lies | 2,440 B, measured |
| Expression bytecode, copied in when the app opens | about 1 KB |
| Section reader for the image on the external flash | 256 B |

That is about 6.6 KB, which leaves the rest of the open app's 12 KB for a second app's state
while a subscription runs. The widgets themselves are allocated from the UI heap.

## Built so far

`neotime-runtime` in `runtime/` is a `no_std` crate with no dependencies. It never allocates, and it
returns an error instead of panicking on any input. It has:

- `image`: reads and checks the header, section table, strings and state variables in place;
- `verifier`: the install-time pass over every expression and handler;
- `state`: lays out an app's variables and arrays in one buffer, each element as wide as the image
  asks;
- `interpreter`: runs expressions and handlers, with locals and loops, keeping a handler's changes
  in working copies until it finishes;
- `text`: the calendar and number formatting;
- `pattern`: the time and duration format patterns;
- `widgets`: reads the widget tree in place, walks screens and children, and checks it;
- `bindings`: works out each expression's inputs and redraw period, which expressions a change
  affects, which are due as the clock moves, and when the next clock-driven redraw is.

Its tests build images by hand, including the stopwatch from [Writing an app](#writing-an-app), and
check that flipping any single byte of an image never panics. Still to come: the subscriptions and
resources sections, the compiler, and the runtime loop that ties these to the UI toolkit and the
system.

## Open questions

- Whether to build the Preact front end first, or start with KDL to get the runtime running.
- How closely the browser preview must match rlvgl's layout before it is useful.
- How screens slide in and out, once the UI toolkit has page transitions.
- Which Bluetooth protocol carries phone data, such as the weather, music and app messages, to the runtime.
- Whether the step limit should be one number or depend on the event.

## Suggested order of work

1. The image format, the verifier and the interpreter as a `no_std` crate, tested on the computer
   with hand-written images. **Done:** `neotime-runtime`.
2. The runtime in the simulator: open an app, draw its widgets, run handlers, redraw on time.
3. The compiler, starting with the stopwatch and a simple watch face, then the browser preview.
4. Persistence through the key-value store, then subscriptions.
5. Installing over BLE, the internal-flash cache, and running on the watch.
