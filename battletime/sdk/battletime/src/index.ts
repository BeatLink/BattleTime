// The battletime package: what an app imports to define itself, keep state, answer events and ask the
// watch for things. docs/app-reference.md lists all of it. On the watch the compiler turns these into
// state slots, system values and effects; in a browser they run as written, for the preview.

import { computed, effect, signal, type ReadonlySignal, type Signal } from "@preact/signals";
import type { ComponentType } from "preact";
import { emit, now_milliseconds, read_stored, storage, write_stored } from "./host.ts";
import type { LibraryIcon } from "./icons.ts";

export { computed, signal, useSignal, type ReadonlySignal, type Signal } from "@preact/signals";

// ============================================================================
// Types
// ============================================================================

/** Milliseconds since 1970. */
export type Instant = number;

/** A length of time in milliseconds. */
export type Duration = number;

export type ThemeColorName = "background" | "surface" | "text" | "muted" | "accent" | "success" | "warning" | "danger";

/** A colour written as `#rrggbb`, or the name of a theme colour; stored on the watch as RGB565. */
export type Color = `#${string}` | ThemeColorName;

export interface TimeOfDay {
  hour: number;
  minute: number;
}

// ============================================================================
// Durations
// ============================================================================

export const SECOND: Duration = 1000;
export const MINUTE: Duration = 60 * SECOND;
export const HOUR: Duration = 60 * MINUTE;
export const DAY: Duration = 24 * HOUR;

// ============================================================================
// Persisted state
// ============================================================================

interface PersistedSlot {
  key: string;
  slot: Signal<unknown>;
  default_value: unknown;
}

const slots_waiting_for_app: PersistedSlot[] = [];

/** A state slot saved to the app's key-value store, so it survives a reboot. */
export function persisted<Value>(key: string, default_value: Value): Signal<Value> {
  const slot = signal(default_value);
  slots_waiting_for_app.push({ key, slot: slot as Signal<unknown>, default_value });
  return slot;
}

function attach_persisted_slots(app_id: string): PersistedSlot[] {
  const slots = slots_waiting_for_app.splice(0);
  for (const { key, slot } of slots) {
    storage.app_id = app_id;
    const stored = read_stored(key);
    if (stored !== undefined) slot.value = stored;
    effect(() => {
      const value = slot.value;
      storage.app_id = app_id;
      write_stored(key, value);
    });
  }
  return slots;
}

// ============================================================================
// System values
// ============================================================================

/** The current time. A binding that reads it redraws as often as its format shows. */
export const now: ReadonlySignal<Instant> = computed(() => now_milliseconds.value);

/** The local time zone's offset, set by the phone. */
export const utc_offset = signal<Duration>(-new Date().getTimezoneOffset() * MINUTE);

export const battery_percent = signal(80);

export const is_charging = signal(false);

/** The battery's smoothed voltage, in millivolts. */
export const battery_millivolts = signal(3900);

/** Steps since local midnight. */
export const steps_today = signal(0);

/** The last heart rate measured, in beats per minute, or 0 if none. */
export const heart_rate = signal(0);

export const heart_rate_measured_at = signal<Instant>(0);

export const is_phone_connected = signal(true);

/** How many of the kept notifications are unread. */
export const unread_notifications = signal(0);

export interface KeptNotification {
  /** What `dismiss_notification` takes; the phone's id, or a negative one for an app's. */
  id: number;
  app: string;
  title: string;
  body: string;
  /** When it arrived. */
  time: Instant;
  /** The icon for its kind, which the watch picks from `app`; see `notification_icon`. */
  icon: LibraryIcon;
  /** The colour for its icon, which the watch picks from `app`; see `notification_color`. */
  color: Color;
}

/** The notifications the watch keeps, newest first, at most 5; reading them needs the `notifications` permission. */
export const notifications = signal<KeptNotification[]>([]);

export const is_do_not_disturb = signal(false);

/** The backlight level, 1 to 3. */
export const brightness = signal(2);

/** The user's choices apps should follow. */
export const settings = {
  uses_24_hour_clock: signal(true),
  uses_metric_units: signal(true),
  step_goal: signal(10000),
  /** Seconds without input before the screen turns off, 5 to 300. */
  screen_timeout_seconds: signal(15),
  /** Whether the screen dims instead of turning off. */
  always_on_screen: signal(false),
  /** 0 off, 1 a buzz every hour, 2 every half hour. */
  chimes: signal(0),
  /** The preset theme in use, from 0 in the watch's order, or -1 for colours of the user's own. */
  theme: signal(0),
  /** Whether the watch takes firmware updates and app installs from a phone: 0 off, 1 on, 2 on until the next restart. */
  allows_updates: signal(1),
  /** What turns a dark screen on besides the button: 0 nothing, 1 a tap, 2 a double tap. */
  wake_on_touch: signal(1),
  /** Whether alarms and timers still ring in do not disturb. */
  alarms_in_do_not_disturb: signal(true),
  /** How often the watch measures the heart rate with no app asking: 0 off, 1 continuously, then every 30 seconds, minute, 5, 10 or 30 minutes. */
  heart_rate_interval: signal(0),
  /** Whether raising the wrist turns a dark screen on. */
  raise_to_wake: signal(false),
  /** Whether lowering the arm turns the screen off. */
  lower_to_sleep: signal(false),
  /** Whether shaking the wrist turns a dark screen on. */
  shake_to_wake: signal(false),
  /** How fast a shake must be to wake the screen, 0 to 4095 as `shake_speed` counts it. */
  shake_threshold: signal(150),
  /** Whether sleep mode is on: do not disturb with every wake gesture and the always-on screen off. */
  sleep_mode: signal(false),
  /** The `number` of the face the watch shows, as the `watch_face` setting holds it; 0 with no face on. */
  watch_face: signal(0),
  /** Whether the watch advertises as "InfiniTime", so Gadgetbridge picks its InfiniTime support; set from the companion app. */
  gadgetbridge_name: signal(false),
};

/** How fast the wrist is shaking, 0 to about 4095, kept current only while a screen shows it. */
export const shake_speed = signal(0);

/** Whether the heart rate sensor is measuring for any app. */
export const is_heart_rate_running = signal(false);

/** The heart rate sensor's state: 0 stopped, 1 no skin, 2 settling, 3 measuring. */
export const heart_rate_status = signal(0);

/** Whether Bluetooth is on, whether or not the phone is connected. */
export const is_bluetooth_enabled = signal(true);

/** The next alarm any app has set to ring, or null. */
export const next_alarm = signal<Instant | null>(null);

/** The name of the watch face the watch shows, or nothing with no face on. */
export const watch_face_name = signal("Digital");

/** The firmware's version and commit, such as `0.1.0+1b19aed`. */
export const firmware_version = signal("0.1.0+preview");

/** When the watch last started, so `now - started_at` is how long it has been running. */
export const started_at = signal<Instant>(now_milliseconds.value);

/** The file system's free space, in kilobytes. */
export const free_storage_kilobytes = signal(2840);

/** Why the watch last restarted, such as `watchdog`, `power on` or `Fault at 0x0001a2b4`. */
export const reset_reason = signal("power on");

/** The day the firmware was built, such as `2026-10-13`. */
export const firmware_build_date = signal("2026-10-13");

/** Whether the bootloader keeps the running firmware: 0 confirmed, 1 on trial until confirmed, 2 rolled back to it after an update failed. */
export const firmware_state = signal(0);

/** The bootloader's version and commit, or `""` when it carries no version record. */
export const bootloader_version = signal("0.1.0+preview");

/** The day the bootloader was built, or `""` when it does not say. */
export const bootloader_build_date = signal("2026-10-11");

/** The bootloader's CRC-32, or null when it could not be read. */
export const bootloader_crc = signal<number | null>(0x1a2b3c4d);

/** The Bluetooth address the watch advertises with, as a 48-bit number: 0xD12A5C7E903B is `D1:2A:5C:7E:90:3B`. */
export const bluetooth_address = signal(0xd12a5c7e903b);

/** The external flash chip's three-byte JEDEC ID, 0x0B4016 for the PineTime's XT25F32B, or 0 when it never answered. */
export const flash_id = signal(0x0b4016);

/** The touch panel's chip ID, 0xB4 for a CST816S, or 0 when it never answered. */
export const touch_chip_id = signal(0xb4);

/** The touch panel's firmware version, or 0 when it never answered. */
export const touch_firmware_version = signal(2);

/** The accelerometer's chip ID, 0x11 for a BMA421 and 0x13 for a BMA425, or 0 when it never answered. */
export const accelerometer_chip_id = signal(0x13);

/** The heap's free bytes when the app read it. */
export const free_heap_bytes = signal(18_432);

/** The fewest free heap bytes since the watch started. */
export const least_free_heap_bytes = signal(12_288);

/** How many allocations the heap has refused since the watch started. */
export const allocation_failures = signal(0);

/** How many times the stack has overflowed into its guard, kept across restarts. */
export const stack_overflows = signal(0);

export interface Task {
  name: string;
  /** How many times the task has run since the watch started. */
  runs: number;
  /** How long it has spent running, in milliseconds. */
  busy_milliseconds: number;
}

/** The firmware's tasks, in the order they started; kept by the system. */
export const tasks = signal<Task[]>([]);

export interface CurrentWeather {
  temperature: number;
  condition: WeatherCondition;
  low: number;
  high: number;
  location: string;
  /** When the forecast arrived, or null before the first. */
  received_at: Instant | null;
  sunrise: Instant | null;
  sunset: Instant | null;
  /** Up to 5 coming days, tomorrow first. */
  forecast: WeatherDay[];
}

/** The last forecast the phone sent; reading it needs no permission. */
export const current_weather = signal<CurrentWeather>({
  temperature: 0,
  condition: "clear",
  low: 0,
  high: 0,
  location: "",
  received_at: null,
  sunrise: null,
  sunset: null,
  forecast: [],
});

export interface InstalledApp {
  /** What `open_app` takes. */
  id: string;
  name: string;
  /** The app's launcher icon, or the shared library's default when it has none. */
  icon: LibraryIcon;
}

/** Every app a launcher lists, in order, group by group; kept by the system. */
export const installed_apps = signal<InstalledApp[]>([]);

/** How the launcher lays out its groups, as the companion set it: one plain list, folders, sections or pages. */
export type LauncherStyle = "list" | "folders" | "sections" | "pages";

/** How the launcher draws a group's apps: a row each, or a grid of icons. */
export type LauncherView = "list" | "grid";

export interface LauncherGroup {
  name: string;
  icon: LibraryIcon;
  /** The place in `installed_apps` of the group's first app. */
  first: number;
  /** How many apps the group holds, which follow each other in `installed_apps`. */
  count: number;
}

/** The launcher style the companion chose; `list` until it chooses one. */
export const launcher_style = signal<LauncherStyle>("list");

/** The launcher view the companion chose; `list` until it chooses one. */
export const launcher_view = signal<LauncherView>("list");

/** The launcher groups holding at least one app, in the order the companion set; kept by the system. */
export const launcher_groups = signal<LauncherGroup[]>([]);

export interface Package {
  /** What `set_app_enabled` takes. */
  id: string;
  name: string;
  is_face: boolean;
  enabled: boolean;
  /** The heap it needs while open, in bytes, as the package manager checks it. */
  memory: number;
  /** Why turning it the other way would be refused, 0 when it would not; docs/app-reference.md lists the numbers. */
  refusal: number;
}

/** Every installed app and face, on or off, in id order; kept by the system. */
export const packages = signal<Package[]>([]);

/** The most heap an open app may need, in bytes. */
export const app_memory_budget = signal(27_648);

/** Files a `TextFile`, `BitmapFile` and `file_bytes` read in the browser preview, by path, standing in for the watch's storage. */
export const preview_files = signal<Record<string, string>>({});

export interface FolderEntry {
  /** The entry's own name, without the folder's path. */
  name: string;
  /** Its size in bytes; a folder's is the sum of every file inside it. */
  size: number;
  is_folder: boolean;
}

export interface Folder {
  /** The folder's path from the top, such as `/apps`, or empty before the first `open_folder`. */
  path: string;
  /** Up to 64 entries, folders first and then files, each by name. */
  entries: FolderEntry[];
}

/** The most entries one folder lists, as the watch keeps them. */
export const MOST_FOLDER_ENTRIES = 64;

/** The longest folder path the watch lists. */
export const LONGEST_FOLDER_PATH_BYTES = 64;

/** The folder the app last opened with `open_folder`; kept by the system. */
export const current_folder = signal<Folder>({ path: "", entries: [] });

/**
 * A folder path written plainly, as the watch writes it: from the top, without empty or `.` parts, with each `..`
 * taking the part before it away and never climbing above the top; null when it is longer than the watch lists.
 */
export function normalise_folder_path(path: string): string | null {
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  const normal = `/${parts.join("/")}`;
  return new TextEncoder().encode(normal).length <= LONGEST_FOLDER_PATH_BYTES ? normal : null;
}

/** A preview file's size: one byte for each character below 256, as raw bytes are kept, and UTF-8 for the rest. */
function preview_file_size(text: string): number {
  let size = 0;
  for (const character of text) {
    const code = character.codePointAt(0) ?? 0;
    size += code < 0x100 ? 1 : new TextEncoder().encode(character).length;
  }
  return size;
}

/** The folder at `path` among the preview's files, listed as the watch lists its storage. */
export function list_preview_folder(files: Record<string, string>, path: string): Folder {
  const normal = normalise_folder_path(path);
  if (normal === null) return { path: "", entries: [] };
  const prefix = normal === "/" ? "/" : `${normal}/`;
  const entries: FolderEntry[] = [];
  for (const [file_path, text] of Object.entries(files)) {
    if (!file_path.startsWith(prefix)) continue;
    const inside = file_path.slice(prefix.length);
    const slash = inside.indexOf("/");
    const name = slash < 0 ? inside : inside.slice(0, slash);
    if (name === "") continue;
    const is_folder = slash >= 0;
    const size = preview_file_size(text);
    const found = entries.find((entry) => entry.name === name && entry.is_folder === is_folder);
    if (found) found.size += size;
    else entries.push({ name, size, is_folder });
  }
  entries.sort((first, second) =>
    first.is_folder !== second.is_folder ? (first.is_folder ? -1 : 1) : first.name < second.name ? -1 : first.name > second.name ? 1 : 0,
  );
  return { path: normal, entries: entries.slice(0, MOST_FOLDER_ENTRIES) };
}

/**
 * Lists a folder of the watch's storage into `current_folder`, soon after the handler ends. The path may be worked
 * out, such as `${current_folder.value.path}/${name}` to go in or `${current_folder.value.path}/..` to go up. The
 * preview lists `preview_files`.
 */
export function open_folder(path: string) {
  emit({ kind: "open_folder", path });
}

/** How many days the step history holds: today and the six before it. */
export const STEP_HISTORY_DAYS = 7;

export interface StepDay {
  total: number;
}

export interface StepHour {
  steps: number;
}

export interface StepHistory {
  /** Steps since the wearer last reset the trip count with `reset_step_trip`. */
  trip: number;
  /** Each day's steps, by how many days back it is: today first, then yesterday. */
  days: StepDay[];
  /** The steps in each hour, at `days_back * 24 + hour`. */
  hours: StepHour[];
}

/** An empty history: seven days of no steps. */
export function empty_step_history(): StepHistory {
  return {
    trip: 0,
    days: Array.from({ length: STEP_HISTORY_DAYS }, () => ({ total: 0 })),
    hours: Array.from({ length: STEP_HISTORY_DAYS * 24 }, () => ({ steps: 0 })),
  };
}

/** The system's step history, kept from local midnight to midnight whether or not any app is open. */
export const step_history = signal<StepHistory>(empty_step_history());

export interface CurrentMusic {
  artist: string;
  track: string;
  is_playing: boolean;
  position: Duration;
  length: Duration;
}

/** What the phone's player last reported, kept by the system; needs the `phone` permission. */
export const current_music = signal<CurrentMusic>({ artist: "", track: "", is_playing: false, position: 0, length: 0 });

export interface CurrentCall {
  /** The caller's name, or their number when the phone has no name. */
  caller: string;
  /** `ended` also when no call has come since the watch started. */
  state: "incoming" | "answered" | "ended";
}

/** The phone's latest call, kept by the system; needs the `phone` permission. */
export const current_call = signal<CurrentCall>({ caller: "", state: "ended" });

export interface CurrentNavigation {
  turn: NavigationTurn;
  instruction: string;
  /** Metres to the turn. */
  distance: number;
  progress_percent: number;
}

/** The phone's latest navigation step, kept by the system; needs the `phone` permission. */
export const current_navigation = signal<CurrentNavigation>({
  turn: "straight",
  instruction: "",
  distance: 0,
  progress_percent: 0,
});

/** The name of the app's screen on display. */
export const current_screen = signal("main");

// ============================================================================
// Events
// ============================================================================

export interface ClickEvent {
  x: number;
  y: number;
}

export interface DragEvent extends ClickEvent {
  start_x: number;
  start_y: number;
}

export type SwipeDirection = "up" | "down" | "left" | "right";

export interface SwipeEvent {
  direction: SwipeDirection;
}

export interface PageChangeEvent {
  /** The page now on show, counted from 0. */
  page: number;
}

export interface ChangeEvent<Value> {
  value: Value;
}

export interface TimeEvent {
  time: Instant;
}

export interface HeartRateEvent {
  beats_per_minute: number;
  confidence: number;
}

export interface StepsEvent {
  steps_today: number;
}

export interface AccelerationEvent {
  /** Thousandths of g along each axis of the watch. */
  x: number;
  y: number;
  z: number;
}

export type MotionKind = "wrist_raise" | "shake" | "tap" | "double_tap" | "still" | "moving";

export interface MotionEvent {
  kind: MotionKind;
}

export type BatteryKind = "low" | "starts" | "stops" | "completes";

export interface BatteryEvent {
  percent: number;
}

export interface PhoneEvent {
  is_connected: boolean;
}

export interface NotificationEvent {
  id: number;
  app: string;
  title: string;
  body: string;
  time: Instant;
}

export interface CallEvent {
  kind: "incoming" | "answered" | "ended";
  caller: string;
}

export interface MusicEvent {
  is_playing: boolean;
  artist: string;
  track: string;
  position: Duration;
  length: Duration;
}

export type WeatherCondition =
  | "clear"
  | "few_clouds"
  | "clouds"
  | "heavy_clouds"
  | "rain"
  | "showers"
  | "thunderstorm"
  | "snow"
  | "mist";

export interface WeatherDay {
  condition: WeatherCondition;
  low: number;
  high: number;
}

export interface WeatherEvent {
  /** Degrees Celsius; apps convert with `settings.uses_metric_units`. */
  temperature: number;
  condition: WeatherCondition;
  low: number;
  high: number;
  location: string;
}

export type NavigationTurn =
  | "straight"
  | "left"
  | "right"
  | "slight_left"
  | "slight_right"
  | "sharp_left"
  | "sharp_right"
  | "u_turn"
  | "roundabout"
  | "arrive";

export interface NavigationEvent {
  turn: NavigationTurn;
  instruction: string;
  /** Metres to the turn. */
  distance: number;
  progress_percent: number;
}

export interface HttpResponseEvent {
  request_id: number;
  status: number;
  body: string;
}

export interface SettingsEvent {
  /** The name of the setting that changed, as `set_setting` takes it. */
  key: string;
}

// ============================================================================
// Subscriptions
// ============================================================================

export interface Period {
  milliseconds?: number;
  seconds?: number;
  minutes?: number;
  hours?: number;
}

export function period_in_milliseconds(period: Period): Duration {
  return (
    (period.milliseconds ?? 0) +
    (period.seconds ?? 0) * SECOND +
    (period.minutes ?? 0) * MINUTE +
    (period.hours ?? 0) * HOUR
  );
}

type Handler<Event> = (event: Event) => void;

/** What the preview runs for an event connected with `on`. */
export type Subscription = { only_while_open: boolean } & (
  | { kind: "every"; period: Period; handler: Handler<TimeEvent> }
  | { kind: "at"; when: ReadonlySignal<Instant | null>; handler: Handler<TimeEvent> }
  | {
      kind: "daily";
      time_of_day: TimeOfDay | ReadonlySignal<TimeOfDay | null>;
      handler: Handler<TimeEvent>;
      days: number | ReadonlySignal<number>;
    }
  | { kind: "acceleration"; period: Period; handler: Handler<AccelerationEvent> }
  | { kind: "clock_change"; unit: "minute" | "hour" | "day"; handler: Handler<TimeEvent> }
  | { kind: "heart_rate_reading"; period: Period; handler: Handler<HeartRateEvent> }
  | { kind: "steps"; handler: Handler<StepsEvent> }
  | { kind: "step_goal"; handler: Handler<StepsEvent> }
  | { kind: "motion"; motion: MotionKind; handler: Handler<MotionEvent> }
  | { kind: "battery"; battery: BatteryKind; handler: Handler<BatteryEvent> }
  | { kind: "phone"; phone: "connects" | "disconnects"; handler: Handler<PhoneEvent> }
  | { kind: "notification"; handler: Handler<NotificationEvent> }
  | { kind: "call"; handler: Handler<CallEvent> }
  | { kind: "music"; handler: Handler<MusicEvent> }
  | { kind: "weather"; handler: Handler<WeatherEvent> }
  | { kind: "navigation"; handler: Handler<NavigationEvent> }
  | { kind: "message"; channel: string; handler: Handler<Record<string, unknown>> }
  | { kind: "http_response"; handler: Handler<HttpResponseEvent> }
  | { kind: "settings_change"; handler: Handler<SettingsEvent> }
);

// ============================================================================
// Events
// ============================================================================

/** Every event `on` can connect to, with what its handler receives. */
export interface EventMap {
  open: undefined;
  close: undefined;
  screen_on: undefined;
  screen_off: undefined;
  tick: TimeEvent;
  time: TimeEvent;
  daily: TimeEvent;
  minute: TimeEvent;
  hour: TimeEvent;
  day: TimeEvent;
  heart_rate: HeartRateEvent;
  steps: StepsEvent;
  step_goal: StepsEvent;
  wrist_raise: MotionEvent;
  shake: MotionEvent;
  wrist_tap: MotionEvent;
  wrist_double_tap: MotionEvent;
  still: MotionEvent;
  moving: MotionEvent;
  acceleration: AccelerationEvent;
  battery_low: BatteryEvent;
  charging_starts: BatteryEvent;
  charging_stops: BatteryEvent;
  charging_completes: BatteryEvent;
  phone_connects: PhoneEvent;
  phone_disconnects: PhoneEvent;
  notification: NotificationEvent;
  call: CallEvent;
  music: MusicEvent;
  weather: WeatherEvent;
  navigation: NavigationEvent;
  settings_change: SettingsEvent;
}

export type EventName = keyof EventMap;

export interface ListenerOptions {
  /** Comes only while the app is open, which needs no `background` permission. */
  while_open?: boolean;
  /** How often `tick`, `heart_rate` and `acceleration` come, such as `MINUTE` or `{ seconds: 1 }`. */
  every?: Duration | Period;
  /** When `time` comes, an Instant or a signal holding one, or the time of day `daily` comes. */
  at?: Instant | ReadonlySignal<Instant | null> | TimeOfDay | ReadonlySignal<TimeOfDay | null>;
  /** The weekdays `daily` comes on, a 7-bit mask with Monday first; every day when left out. */
  days?: number;
  /** Skips the handler until this long has passed since it last ran, such as `SECOND`. */
  at_most_every?: Duration | Period;
}

/** A lifecycle event's handler, kept until `define_app` collects it. */
interface LifecycleListener {
  event: "open" | "close" | "screen_on" | "screen_off";
  handler: () => void;
}

let waiting_subscriptions: Subscription[] = [];
let waiting_lifecycle: LifecycleListener[] = [];

const MOTION_EVENTS: Partial<Record<EventName, MotionKind>> = {
  wrist_raise: "wrist_raise",
  shake: "shake",
  wrist_tap: "tap",
  wrist_double_tap: "double_tap",
  still: "still",
  moving: "moving",
};

const BATTERY_EVENTS: Partial<Record<EventName, BatteryKind>> = {
  battery_low: "low",
  charging_starts: "starts",
  charging_stops: "stops",
  charging_completes: "completes",
};

function as_period(duration: Duration | Period | undefined): Period {
  return typeof duration === "number" ? { milliseconds: duration } : duration ?? {};
}

function as_signal<Value>(value: Value | ReadonlySignal<Value>): ReadonlySignal<Value> {
  return typeof value === "object" && value !== null && "peek" in value ? (value as ReadonlySignal<Value>) : signal(value as Value);
}

/** The handler wrapped so it runs at most once every `pause`, by the watch's clock. */
function at_most_every<Event>(pause: Duration | Period, handler: Handler<Event>): Handler<Event> {
  const milliseconds = typeof pause === "number" ? pause : period_in_milliseconds(pause);
  let last_run = 0;
  return (event) => {
    const current = now.peek();
    if (current - last_run < milliseconds) return;
    last_run = current;
    handler(event);
  };
}

/** Connects a handler to an event, at the top of the file and before `define_app`. */
export function on<Name extends EventName>(event: Name, handler: Handler<EventMap[Name]>, options: ListenerOptions = {}) {
  const run = options.at_most_every === undefined ? handler : at_most_every(options.at_most_every, handler);
  if (event === "open" || event === "close" || event === "screen_on" || event === "screen_off") {
    waiting_lifecycle.push({ event, handler: run as () => void });
    return;
  }
  const only_while_open = options.while_open ?? false;
  // Each subscription's handler type follows from its kind, which TypeScript cannot see through `event`.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const handle = run as Handler<any>;
  const motion_kind = MOTION_EVENTS[event];
  const battery_kind = BATTERY_EVENTS[event];
  let subscription: Subscription;
  if (motion_kind) subscription = { kind: "motion", motion: motion_kind, handler: handle, only_while_open };
  else if (battery_kind) subscription = { kind: "battery", battery: battery_kind, handler: handle, only_while_open };
  else {
    switch (event as EventName) {
      case "tick":
        subscription = { kind: "every", period: as_period(options.every), handler: handle, only_while_open };
        break;
      case "heart_rate":
        subscription = { kind: "heart_rate_reading", period: as_period(options.every), handler: handle, only_while_open };
        break;
      case "acceleration":
        subscription = { kind: "acceleration", period: as_period(options.every), handler: handle, only_while_open };
        break;
      case "time":
        subscription = { kind: "at", when: as_signal(options.at as Instant | null), handler: handle, only_while_open };
        break;
      case "daily":
        subscription = {
          kind: "daily",
          time_of_day: options.at as TimeOfDay | ReadonlySignal<TimeOfDay | null>,
          days: options.days ?? EVERY_DAY,
          handler: handle,
          only_while_open,
        };
        break;
      case "minute":
      case "hour":
      case "day":
        subscription = { kind: "clock_change", unit: event as "minute" | "hour" | "day", handler: handle, only_while_open };
        break;
      case "phone_connects":
      case "phone_disconnects":
        subscription = {
          kind: "phone",
          phone: event === "phone_connects" ? "connects" : "disconnects",
          handler: handle,
          only_while_open,
        };
        break;
      case "steps":
        subscription = { kind: "steps", handler: handle, only_while_open };
        break;
      case "step_goal":
        subscription = { kind: "step_goal", handler: handle, only_while_open };
        break;
      case "notification":
        subscription = { kind: "notification", handler: handle, only_while_open };
        break;
      case "call":
        subscription = { kind: "call", handler: handle, only_while_open };
        break;
      case "music":
        subscription = { kind: "music", handler: handle, only_while_open };
        break;
      case "weather":
        subscription = { kind: "weather", handler: handle, only_while_open };
        break;
      case "navigation":
        subscription = { kind: "navigation", handler: handle, only_while_open };
        break;
      default:
        subscription = { kind: "settings_change", handler: handle, only_while_open };
    }
  }
  waiting_subscriptions.push(subscription);
}

/** Hands `define_app` every listener connected since the last app was defined. */
function take_listeners(): { subscriptions: Subscription[]; lifecycle: LifecycleListener[] } {
  const taken = { subscriptions: waiting_subscriptions, lifecycle: waiting_lifecycle };
  waiting_subscriptions = [];
  waiting_lifecycle = [];
  return taken;
}

// ============================================================================
// Effects
// ============================================================================

export type VibrationPattern = "tap" | "double_tap" | "notification" | "alarm" | "success" | "failure";
export type MusicCommand = "play" | "pause" | "next" | "previous" | "volume_up" | "volume_down";
export type CallCommand = "answer" | "reject" | "mute";

/** Runs the motor once, for at most 2 seconds. */
export function vibrate(milliseconds: number) {
  emit({ kind: "vibrate", milliseconds: Math.min(milliseconds, 2000) });
}

/** A named pattern; `alarm` repeats until `stop_vibrating` or the app closes. */
export function vibrate_pattern(pattern: VibrationPattern) {
  emit({ kind: "vibrate_pattern", pattern });
}

/** A custom pattern of on and off times in milliseconds, up to 16 steps. */
export function vibrate_sequence(steps_in_milliseconds: number[]) {
  emit({ kind: "vibrate_sequence", steps_in_milliseconds: steps_in_milliseconds.slice(0, 16) });
}

export function stop_vibrating() {
  emit({ kind: "stop_vibrating" });
}

/** Sets the backlight, 1 to 3, until the app closes. */
export function set_brightness(level: number) {
  emit({ kind: "set_brightness", level });
}

export function keep_screen_on(on: boolean) {
  emit({ kind: "keep_screen_on", on });
}

/** Turns the screen on and brings this app to the front, such as for an alarm. */
export function wake_screen() {
  emit({ kind: "wake_screen" });
}

export function screen_off() {
  emit({ kind: "screen_off" });
}

export function go(screen: string) {
  emit({ kind: "go", screen });
}

/** Returns to the previous screen, or leaves the app from its first one. */
export function back() {
  emit({ kind: "back" });
}

export function open_app(id: string) {
  emit({ kind: "open_app", id });
}

/** Leaves the app for the watch face. */
export function close() {
  emit({ kind: "close" });
}

export function show_toast(text: string) {
  emit({ kind: "show_toast", text });
}

export function notify(title: string, body: string) {
  emit({ kind: "notify", title, body });
}

export function dismiss_notification(id: number) {
  emit({ kind: "dismiss_notification", id });
}

export function mark_notifications_read() {
  emit({ kind: "mark_notifications_read" });
}

export function send_to_phone(channel: string, data: Record<string, unknown>) {
  emit({ kind: "send_to_phone", channel, data });
}

export function http_request(request_id: number, method: string, url: string, body: string) {
  emit({ kind: "http_request", request_id, method, url, body });
}

export function music_control(command: MusicCommand) {
  emit({ kind: "music_control", command });
}

export function call_control(command: CallCommand) {
  emit({ kind: "call_control", command });
}

/** Makes the phone ring, or stops it. */
export function find_phone(on: boolean) {
  emit({ kind: "find_phone", on });
}

/** Asks the phone to send its navigation app's current step now. */
export function request_navigation_state() {
  emit({ kind: "request_navigation_state" });
}

/** Asks the phone to send its player's state now, through the `music` subscription. */
export function request_music_state() {
  emit({ kind: "request_music_state" });
}

/** Drives the heart rate sensor's LED directly, until the app closes. */
export function set_heart_rate_led(on: boolean, brightness: number, period: number) {
  emit({ kind: "set_heart_rate_led", on, brightness, period });
}

export function set_bluetooth(on: boolean) {
  emit({ kind: "set_bluetooth", on });
}

/** Turns an installed app or face on or off; the watch does nothing when its `refusal` says no. Needs `settings`. */
export function set_app_enabled(id: string, on: boolean) {
  emit({ kind: "set_app_enabled", id, on });
}

/** Opens the face chooser: each face turned on full screen, swiped left and right, kept on going back. Needs `settings`. */
export function choose_watch_face() {
  emit({ kind: "choose_watch_face" });
}

/** Takes one heart rate reading now. */
export function measure_heart_rate() {
  emit({ kind: "measure_heart_rate" });
}

/** Measures continuously until stopped or the app closes. */
export function start_heart_rate() {
  emit({ kind: "start_heart_rate" });
}

export function stop_heart_rate() {
  emit({ kind: "stop_heart_rate" });
}

/** Tells the bootloader to keep a firmware on trial, so a restart no longer rolls it back. Needs `settings`. */
export function confirm_firmware() {
  emit({ kind: "confirm_firmware" });
}

/** Restarts a firmware on trial without confirming it, so the bootloader brings back the one before. Needs `settings`. */
export function roll_back_firmware() {
  emit({ kind: "roll_back_firmware" });
}

/** Starts the trip count in `step_history` again from zero. */
export function reset_step_trip() {
  emit({ kind: "reset_step_trip" });
}

/** Changes one of `settings`, `brightness`, `do_not_disturb` or `bluetooth` by name. Booleans are 1 and 0. */
export function set_setting(key: string, value: number) {
  emit({ kind: "set_setting", key, value });
}

export function set_do_not_disturb(on: boolean) {
  emit({ kind: "set_do_not_disturb", on });
}

/** Sets the watch's clock to a local time in its zone, with the seconds at 00: a day of the year from 1 and a minute of the day from 0. Needs `settings`. */
export function set_clock(year: number, day_of_year: number, minute_of_day: number) {
  emit({ kind: "set_clock", year, day_of_year, minute_of_day });
}

export function log(text: string) {
  emit({ kind: "log", text });
}

// ============================================================================
// Numbers, time and text
// ============================================================================

/** A random whole number from `lowest` to `highest`, both included. Handlers only. */
export function random_int(lowest: number, highest: number): number {
  return lowest + Math.floor(Math.random() * (highest - lowest + 1));
}

export function clamp(value: number, lowest: number, highest: number): number {
  return Math.min(highest, Math.max(lowest, value));
}

function pad(value: number, width: number): string {
  return String(Math.trunc(value)).padStart(width, "0");
}

/**
 * Replaces each token in a pattern with its part, copies `[bracketed]` text and non-letters as they are,
 * and rejects any other letter, as the watch does when the app is installed.
 */
function fill_pattern(pattern: string, tokens: RegExp, parts: Record<string, string>): string {
  const matcher = new RegExp(`\\[([^\\]]*)\\]|${tokens.source}|[A-Za-z]`, "g");
  return pattern.replace(matcher, (token, literal: string | undefined) => {
    if (literal !== undefined) return literal;
    if (!(token in parts)) throw new Error(`"${token}" is not a token in the pattern "${pattern}"`);
    return parts[token];
  });
}

/**
 * Formats a duration with `hh`, `h`, `mm`, `m`, `ss`, `s` and `cc` (hundredths).
 * The largest unit in the pattern shows the whole amount, so `mm:ss` shows 75 minutes as `75:00`.
 */
export function format_duration(duration: Duration, pattern: string): string {
  const total = Math.max(0, Math.floor(duration));
  const without_literals = pattern.replace(/\[[^\]]*\]/g, "");
  const has_hours = /h/.test(without_literals);
  const has_minutes = /m/.test(without_literals);
  const hours = Math.floor(total / HOUR);
  const minutes = has_hours ? Math.floor(total / MINUTE) % 60 : Math.floor(total / MINUTE);
  const seconds = has_minutes || has_hours ? Math.floor(total / SECOND) % 60 : Math.floor(total / SECOND);
  const hundredths = Math.floor(total / 10) % 100;
  return fill_pattern(pattern, /hh|h|mm|m|ss|s|cc/, {
    hh: pad(hours, 2),
    h: String(hours),
    mm: pad(minutes, 2),
    m: String(minutes),
    ss: pad(seconds, 2),
    s: String(seconds),
    cc: pad(hundredths, 2),
  });
}

const WEEKDAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/**
 * Formats the local time of an instant with the named patterns `clock`, `clock_with_seconds` and `date`,
 * or tokens: YYYY, MMMM, MMM, MM, M, DD, D, dddd, ddd, HH, H, hh, h (12 hour), mm, m, ss, s, a and A.
 */
export function format_time(instant: Instant, pattern: string): string {
  const uses_24_hour_clock = settings.uses_24_hour_clock.value;
  if (pattern === "clock") pattern = uses_24_hour_clock ? "HH:mm" : "h:mm";
  if (pattern === "clock_with_seconds") pattern = uses_24_hour_clock ? "HH:mm:ss" : "h:mm:ss";
  if (pattern === "date") pattern = "YYYY-MM-DD";
  const date = new Date(instant);
  const hours = date.getHours();
  const twelve_hour = hours % 12 === 0 ? 12 : hours % 12;
  return fill_pattern(pattern, /YYYY|MMMM|MMM|MM|M|DD|D|dddd|ddd|HH|H|hh|h|mm|m|ss|s|a|A/, {
    YYYY: String(date.getFullYear()),
    MMMM: MONTH_NAMES[date.getMonth()],
    MMM: MONTH_NAMES[date.getMonth()].slice(0, 3),
    MM: pad(date.getMonth() + 1, 2),
    M: String(date.getMonth() + 1),
    DD: pad(date.getDate(), 2),
    D: String(date.getDate()),
    dddd: WEEKDAY_NAMES[date.getDay()],
    ddd: WEEKDAY_NAMES[date.getDay()].slice(0, 3),
    HH: pad(hours, 2),
    H: String(hours),
    hh: pad(twelve_hour, 2),
    h: String(twelve_hour),
    mm: pad(date.getMinutes(), 2),
    m: String(date.getMinutes()),
    ss: pad(date.getSeconds(), 2),
    s: String(date.getSeconds()),
    a: hours < 12 ? "am" : "pm",
    A: hours < 12 ? "AM" : "PM",
  });
}
/** The text in capitals, such as `MON` from a pattern's names. */
export function to_upper(text: string): string {
  return text.toUpperCase();
}

/** The text with spaces, or the character given, added in front up to `width`. */
export function pad_start(text: string, width: number, character = " "): string {
  return text.padStart(width, character);
}

export function text_length(text: string): number {
  return text.length;
}

/** Part of the text, counted in characters. */
export function slice_text(text: string, start: number, end?: number): string {
  return text.slice(start, end);
}

/** The texts of a fixed-length array joined together. */
export function join(texts: string[], separator: string): string {
  return texts.join(separator);
}

/** Text from the first `length` character codes of an array, for text kept in state as numbers. */
export function text_from_codes(codes: number[], length: number): string {
  return String.fromCharCode(...codes.slice(0, length));
}

/** `0` to `count - 1`, where `count` is known when the app is compiled. */
export function range(count: number): number[] {
  return Array.from({ length: count }, (_, index) => index);
}

/** How many elements pass the test. */
export function count_where<Element>(array: Element[], test: (element: Element) => boolean): number {
  return array.filter(test).length;
}

/** How many elements equal `value`. The watch counts natively, so no loop is carried in the app. */
export function count_of(array: readonly number[], value: number): number {
  return array.filter((element) => element === value).length;
}

/** The index of the `n`th element equal to `value`, counting from 0, or -1 when there are fewer. */
export function nth_index_of(array: readonly number[], value: number, n: number): number {
  let seen = 0;
  for (let index = 0; index < array.length; index++) {
    if (array[index] !== value) continue;
    if (seen === n) return index;
    seen++;
  }
  return -1;
}

/** Whether two arrays of the same length hold the same elements. */
export function arrays_equal(first: readonly number[], second: readonly number[]): boolean {
  return first.length === second.length && first.every((element, index) => element === second[index]);
}

/**
 * The longest line of `value` through `cell` on a grid `width` cells wide, across, down or along either
 * diagonal, counting the cell itself as holding the value. Lines stop at the grid's edges.
 */
export function line_through(grid: readonly number[], width: number, cell: number, value: number): number {
  const rows = grid.length / width;
  const row = Math.floor(cell / width);
  const column = cell % width;
  let longest = 0;
  for (const [row_step, column_step] of [[0, 1], [1, 0], [1, 1], [1, -1]]) {
    let length = 1;
    for (const sign of [1, -1]) {
      for (let distance = 1; ; distance++) {
        const next_row = row + sign * row_step * distance;
        const next_column = column + sign * column_step * distance;
        if (next_row < 0 || next_row >= rows || next_column < 0 || next_column >= width) break;
        if (grid[next_row * width + next_column] !== value) break;
        length++;
      }
    }
    longest = Math.max(longest, length);
  }
  return longest;
}

/** The array moved one place towards its start, with `value` in the last place: a history, newest last. Assign it to an array signal. */
export function push_shift<Element>(array: readonly Element[], value: Element): Element[] {
  return [...array.slice(1), value];
}

/** The array moved one place towards its end, with `value` in the first place: a history, newest first. Assign it to an array signal. */
export function unshift_pop<Element>(array: readonly Element[], value: Element): Element[] {
  return [value, ...array.slice(0, -1)];
}

/**
 * `length` bytes of a file on the watch from `offset`, its first bytes without one, 0 to 255 each, with 0 past its
 * end or when it is missing. The path is written out and the length known when compiling; the offset may be worked
 * out, so one array can page through a large file. Assign it to an array signal of that length in a handler, such
 * as `onOpen`. After `prefetch_file` of the same part the bytes are already in RAM. The preview reads the file from
 * `preview_files` as UTF-8.
 */
export function file_bytes(path: string, length: number, offset = 0): number[] {
  const bytes = new TextEncoder().encode(preview_files.value[path] ?? "");
  return Array.from({ length }, (_, index) => bytes[offset + index] ?? 0);
}

/**
 * Starts reading `length` bytes of a file from `offset` into RAM in the background, at most 4,096, so the screen
 * keeps drawing while they arrive and a later `file_bytes` of that part takes them without waiting for the flash.
 * One part is held at a time, until a `file_bytes` takes it or the next `prefetch_file` replaces it.
 */
export function prefetch_file(path: string, offset: number, length: number) {
  emit({ kind: "prefetch_file", path, offset, length });
}

/**
 * A number kept in units of a `places`-digit fraction, as text: `format_fixed(1250, 3)` is `1.250`. With
 * `trim`, trailing zero decimals are left off, and the point too when none are left.
 */
export function format_fixed(value: number, places: number, trim = false): string {
  const scale = 10 ** places;
  const magnitude = Math.abs(value);
  let fraction = places > 0 ? `${magnitude % scale}`.padStart(places, "0") : "";
  if (trim) fraction = fraction.replace(/0+$/, "");
  const sign = value < 0 ? "-" : "";
  return `${sign}${Math.floor(magnitude / scale)}${fraction.length > 0 ? "." : ""}${fraction}`;
}

/** How many pairs of elements are out of order, the earlier one greater. Its parity tells whether a sliding puzzle can be solved. */
export function count_inversions(array: readonly number[]): number {
  let inversions = 0;
  array.forEach((value, earlier) => {
    for (let later = earlier + 1; later < array.length; later++) if (array[later] < value) inversions++;
  });
  return inversions;
}

/** The array with the `count` elements from `first` on in a random order. Assign it to an array signal. */
export function shuffled<Element>(array: readonly Element[], first: number, count: number): Element[] {
  const result = [...array];
  for (let place = count - 1; place > 0; place--) {
    const other = random_int(0, place);
    [result[first + place], result[first + other]] = [result[first + other], result[first + place]];
  }
  return result;
}

/** A whole number padded with zeros to at least `width` digits. */
export function format_number(value: number, width = 1): string {
  const sign = value < 0 ? "-" : "";
  return sign + pad(Math.abs(value), width);
}

export function hour_of(instant: Instant): number {
  return new Date(instant).getHours();
}

export function minute_of(instant: Instant): number {
  return new Date(instant).getMinutes();
}

export function second_of(instant: Instant): number {
  return new Date(instant).getSeconds();
}

/** The day of the week, 0 for Monday to 6 for Sunday. */
export function weekday_of(instant: Instant): number {
  return (new Date(instant).getDay() + 6) % 7;
}

export function day_of(instant: Instant): number {
  return new Date(instant).getDate();
}

/** The month, 1 for January to 12. */
export function month_of(instant: Instant): number {
  return new Date(instant).getMonth() + 1;
}

export function year_of(instant: Instant): number {
  return new Date(instant).getFullYear();
}

/** The local midnight that starts the day an instant falls in. */
export function start_of_day(instant: Instant): Instant {
  const date = new Date(instant);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** The start of the minute an instant falls in; a binding using it redraws once a minute. */
export function start_of_minute(instant: Instant): Instant {
  return Math.floor(instant / MINUTE) * MINUTE;
}

/** The start of the local hour an instant falls in; a binding using it redraws once an hour. */
export function start_of_hour(instant: Instant): Instant {
  const date = new Date(instant);
  date.setMinutes(0, 0, 0);
  return date.getTime();
}

/** The day of the year, 1 to 366. */
export function day_of_year(instant: Instant): number {
  const date = new Date(instant);
  const first_of_january = new Date(date.getFullYear(), 0, 1);
  return Math.round((start_of_day(instant) - first_of_january.getTime()) / DAY) + 1;
}

/** The ISO 8601 week number, 1 to 53. */
export function iso_week_of(instant: Instant): number {
  const date = new Date(start_of_day(instant));
  date.setDate(date.getDate() + 3 - ((date.getDay() + 6) % 7));
  const first_thursday_week = new Date(date.getFullYear(), 0, 4);
  return 1 + Math.round(((date.getTime() - first_thursday_week.getTime()) / DAY - 3 + ((first_thursday_week.getDay() + 6) % 7)) / 7);
}

/** Every weekday in a `days` mask, which has Monday as its lowest bit. */
export const EVERY_DAY = 0b1111111;

/** The first instant after `after` at a local time of day, on one of the weekdays in `days`. */
export function next_time_of_day(after: Instant, time_of_day: TimeOfDay, days: number = EVERY_DAY): Instant {
  const offset = (time_of_day.hour * 60 + time_of_day.minute) * MINUTE;
  let day = start_of_day(after);
  for (const _ of [0, 1, 2, 3, 4, 5, 6, 7]) {
    const candidate = day + offset;
    if (candidate > after && (days & (1 << weekday_of(candidate))) !== 0) return candidate;
    day = start_of_day(day + DAY + 2 * HOUR);
  }
  return day + offset;
}

/** A temperature in the user's units, with its sign, such as `21°C`. */
export function format_temperature(celsius: number): string {
  return settings.uses_metric_units.value ? `${celsius}°C` : `${Math.round((celsius * 9) / 5) + 32}°F`;
}

// ============================================================================
// Apps
// ============================================================================

export type Permission =
  | "heart_rate"
  | "motion"
  | "notifications"
  | "phone"
  | "phone_messages"
  | "internet"
  | "settings"
  | "wake"
  | "background";

export type ScreenComponent = ComponentType;

export interface AppOptions {
  id: string;
  name: string;
  icon: LibraryIcon | string;
  /** An app, the default, or a watch face, which has one screen and cannot take the button. */
  kind?: "app" | "face";
  /** The launcher group the app goes in until the wearer moves it, such as `"games"`; docs/apps.md lists the usual ones. */
  group?: string;
  screens: { main: ScreenComponent } & Record<string, ScreenComponent>;
}

/** An app as the preview runs it, with the events its `on` calls connected. */
export interface App extends AppOptions {
  kind: "app" | "face";
  subscriptions: Subscription[];
  onOpen?: () => void;
  onClose?: () => void;
  onScreenOff?: () => void;
  onScreenOn?: () => void;
}

const persisted_slots_of_app = new WeakMap<App, PersistedSlot[]>();

/** Declares an app: who it is, its screens and the work it wants done in the background. */
export function define_app(options: AppOptions): App {
  const slots = attach_persisted_slots(options.id);
  const listeners = take_listeners();
  const app: App = { kind: "app", ...options, subscriptions: listeners.subscriptions };
  const hooks = { open: "onOpen", close: "onClose", screen_on: "onScreenOn", screen_off: "onScreenOff" } as const;
  for (const { event, handler } of listeners.lifecycle) {
    const earlier = app[hooks[event]];
    app[hooks[event]] = earlier ? () => (earlier(), handler()) : handler;
  }
  persisted_slots_of_app.set(app, slots);
  return app;
}

/** Puts every persisted slot of an app back to its default, as a fresh install has it. */
export function reset_persisted(app: App) {
  for (const { slot, default_value } of persisted_slots_of_app.get(app) ?? []) {
    slot.value = default_value;
  }
}
