// The neotime package: what an app imports to define itself, keep state, answer events and ask the
// watch for things. docs/app-reference.md lists all of it. On the watch the compiler turns these into
// state slots, system values and effects; in a browser they run as written, for the preview.

import { computed, effect, signal, type ReadonlySignal, type Signal } from "@preact/signals";
import type { ComponentType } from "preact";
import { emit, now_milliseconds, read_stored, storage, write_stored } from "./host.ts";
import type { LibraryIcon } from "./icons.ts";

export type { ReadonlySignal, Signal } from "@preact/signals";

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

/** Steps since local midnight. */
export const steps_today = signal(0);

/** The last heart rate measured, in beats per minute, or 0 if none. */
export const heart_rate = signal(0);

export const heart_rate_measured_at = signal<Instant>(0);

export const is_phone_connected = signal(true);

export const unread_notifications = signal(0);

export const is_do_not_disturb = signal(false);

/** The backlight level, 1 to 3. */
export const brightness = signal(2);

/** The user's choices apps should follow. */
export const settings = {
  uses_24_hour_clock: signal(true),
  uses_metric_units: signal(true),
  step_goal: signal(10000),
};

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

export interface ButtonEvent {
  kind: "press" | "long_press";
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
  /** Up to 5 coming days. */
  days: WeatherDay[];
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
  distance: string;
  progress_percent: number;
}

export interface HttpResponseEvent {
  request_id: number;
  status: number;
  body: string;
}

export interface SettingsEvent {
  keys: string[];
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

export type Subscription =
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
  | { kind: "settings_change"; handler: Handler<SettingsEvent> };

/** Repeatedly; periods shorter than a minute run only while the app is open. */
export function every(period: Period, handler: Handler<TimeEvent>): Subscription {
  return { kind: "every", period, handler };
}

/** Once, at an absolute time; changing the signal reschedules it, and null cancels it. */
export function at(when: ReadonlySignal<Instant | null>, handler: Handler<TimeEvent>): Subscription {
  return { kind: "at", when, handler };
}

/** At a local time on the weekdays in `days` (Monday first, every day when left out); null turns it off. */
export function daily(
  time_of_day: TimeOfDay | ReadonlySignal<TimeOfDay | null>,
  handler: Handler<TimeEvent>,
  days: number | ReadonlySignal<number> = EVERY_DAY,
): Subscription {
  return { kind: "daily", time_of_day, handler, days };
}

/** Raw accelerometer readings, only while the app is open and at most 10 a second. */
export function acceleration(period: Period, handler: Handler<AccelerationEvent>): Subscription {
  return { kind: "acceleration", period, handler };
}

/** When the local minute, hour or day changes, and when the phone sets the clock. */
export function clock_change(unit: "minute" | "hour" | "day", handler: Handler<TimeEvent>): Subscription {
  return { kind: "clock_change", unit, handler };
}

/** A heart rate measurement every period, shared with any other app asking. */
export function heart_rate_reading(period: Period, handler: Handler<HeartRateEvent>): Subscription {
  return { kind: "heart_rate_reading", period, handler };
}

/** The step count changes, at most once a minute. */
export function steps(handler: Handler<StepsEvent>): Subscription {
  return { kind: "steps", handler };
}

/** The day's step goal is reached. */
export function step_goal(handler: Handler<StepsEvent>): Subscription {
  return { kind: "step_goal", handler };
}

export function motion(kind: MotionKind, handler: Handler<MotionEvent>): Subscription {
  return { kind: "motion", motion: kind, handler };
}

export function battery(kind: BatteryKind, handler: Handler<BatteryEvent>): Subscription {
  return { kind: "battery", battery: kind, handler };
}

export function phone(kind: "connects" | "disconnects", handler: Handler<PhoneEvent>): Subscription {
  return { kind: "phone", phone: kind, handler };
}

export function notification(handler: Handler<NotificationEvent>): Subscription {
  return { kind: "notification", handler };
}

export function call(handler: Handler<CallEvent>): Subscription {
  return { kind: "call", handler };
}

/** The phone's music player changes. */
export function music(handler: Handler<MusicEvent>): Subscription {
  return { kind: "music", handler };
}

/** The phone sends a forecast. */
export function weather(handler: Handler<WeatherEvent>): Subscription {
  return { kind: "weather", handler };
}

/** The phone's navigation app gives the next turn. */
export function navigation(handler: Handler<NavigationEvent>): Subscription {
  return { kind: "navigation", handler };
}

/** The companion app sends this app a message on its own channel. */
export function message(channel: string, handler: Handler<Record<string, unknown>>): Subscription {
  return { kind: "message", channel, handler };
}

export function http_response(handler: Handler<HttpResponseEvent>): Subscription {
  return { kind: "http_response", handler };
}

/** The user changes a setting in `settings`. */
export function settings_change(handler: Handler<SettingsEvent>): Subscription {
  return { kind: "settings_change", handler };
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

export function find_phone() {
  emit({ kind: "find_phone" });
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

export function reset_steps() {
  emit({ kind: "reset_steps" });
}

export function set_setting(key: string, value: unknown) {
  emit({ kind: "set_setting", key, value });
}

export function set_do_not_disturb(on: boolean) {
  emit({ kind: "set_do_not_disturb", on });
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
  screens: { main: ScreenComponent } & Record<string, ScreenComponent>;
  subscriptions?: Subscription[];
  permissions?: Permission[];
  onOpen?: () => void;
  onClose?: () => void;
  onScreenOff?: () => void;
  onScreenOn?: () => void;
}

export interface App extends AppOptions {
  kind: "app" | "face";
  subscriptions: Subscription[];
  permissions: Permission[];
}

const persisted_slots_of_app = new WeakMap<App, PersistedSlot[]>();

/** Declares an app: who it is, its screens and the work it wants done in the background. */
export function define_app(options: AppOptions): App {
  const slots = attach_persisted_slots(options.id);
  const app: App = { kind: "app", subscriptions: [], permissions: [], ...options };
  persisted_slots_of_app.set(app, slots);
  return app;
}

/** Puts every persisted slot of an app back to its default, as a fresh install has it. */
export function reset_persisted(app: App) {
  for (const { slot, default_value } of persisted_slots_of_app.get(app) ?? []) {
    slot.value = default_value;
  }
}
