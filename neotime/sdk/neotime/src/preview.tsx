// The browser preview's stand-in for the runtime: it shows an app's screens, carries out its effects
// and runs its subscriptions from the clock. Apps never import this module.

import { batch, effect, signal } from "@preact/signals";
import { clock, emit, listen_for_effects, now_milliseconds, start_clock, type Effect } from "./host.ts";
import {
  current_screen,
  current_call,
  current_music,
  current_navigation,
  current_weather,
  heart_rate,
  heart_rate_status,
  is_bluetooth_enabled,
  is_heart_rate_running,
  heart_rate_measured_at,
  brightness,
  is_do_not_disturb,
  notifications,
  packages,
  period_in_milliseconds,
  hour_of,
  settings,
  start_of_day,
  step_history,
  steps_today,
  unread_notifications,
  watch_face_name,
  DAY,
  HOUR,
  MINUTE,
  STEP_HISTORY_DAYS,
  type App,
  type Instant,
  type KeptNotification,
  type Subscription,
  type TimeOfDay,
  type WeatherDay,
} from "./index.ts";
import { run_handler } from "./widgets.tsx";
import { notification_icon } from "./icons.ts";

export { clear_storage } from "./host.ts";
export { swipe } from "./widgets.tsx";

// ============================================================================
// Open app and screens
// ============================================================================

export const open_app = signal<App | null>(null);
export const screen_stack = signal<string[]>(["main"]);
export const is_screen_on = signal(true);
export const toast = signal<string | null>(null);
export const effect_log = signal<Effect[]>([]);
export const is_vibrating = signal(false);

let installed_apps: App[] = [];

/** The apps the preview knows about, whose background subscriptions it runs. */
export function install_apps(apps: App[]) {
  installed_apps = apps;
}

export function open(app: App | null) {
  const previous = open_app.peek();
  if (previous === app) return;
  if (previous) run_app_handler(previous, previous.onClose, undefined);
  batch(() => {
    open_app.value = app;
    screen_stack.value = ["main"];
    current_screen.value = "main";
    is_screen_on.value = true;
  });
  if (app) run_app_handler(app, app.onOpen, undefined);
}

function show_screen(stack: string[]) {
  batch(() => {
    screen_stack.value = stack;
    current_screen.value = stack[stack.length - 1];
  });
}

// ============================================================================
// Effects
// ============================================================================

let emitting_app: App | null = null;

/** Runs one of an app's handlers, so effects it asks for are carried out for that app. */
export function run_app_handler<Event>(app: App, handler: ((event: Event) => void) | undefined, event: Event) {
  const previous = emitting_app;
  emitting_app = app;
  try {
    run_handler(handler, event);
  } finally {
    emitting_app = previous;
  }
}

let toast_timer: ReturnType<typeof setTimeout> | null = null;
let vibration_timer: ReturnType<typeof setTimeout> | null = null;

function show_toast(text: string) {
  toast.value = text;
  if (toast_timer) clearTimeout(toast_timer);
  toast_timer = setTimeout(() => (toast.value = null), 2000);
}

function buzz(milliseconds: number, repeat = false) {
  is_vibrating.value = true;
  if (vibration_timer) clearTimeout(vibration_timer);
  if (!repeat) vibration_timer = setTimeout(() => (is_vibrating.value = false), milliseconds);
}

function carry_out(effect: Effect) {
  const app = emitting_app ?? open_app.peek();
  effect_log.value = [...effect_log.peek().slice(-49), effect];
  const stack = screen_stack.peek();
  switch (effect.kind) {
    case "go":
      if (app === open_app.peek()) show_screen([...stack, effect.screen as string]);
      break;
    case "back":
      if (stack.length > 1) show_screen(stack.slice(0, -1));
      else open(null);
      break;
    case "close":
      open(null);
      break;
    case "open_app":
      open(installed_apps.find((candidate) => candidate.id === effect.id) ?? null);
      break;
    case "wake_screen":
      if (app) open(app);
      is_screen_on.value = true;
      break;
    case "screen_off":
      is_screen_on.value = false;
      break;
    case "show_toast":
      show_toast(effect.text as string);
      break;
    case "notify":
      keep_notification({
        id: next_app_notification_id--,
        app: app?.name ?? "",
        title: effect.title as string,
        body: effect.body as string,
        time: now_milliseconds.peek(),
      });
      show_toast(`${effect.title}: ${effect.body}`);
      buzz(200);
      break;
    case "dismiss_notification":
      kept_notifications = kept_notifications.filter((kept) => kept.notification.id !== effect.id);
      show_notifications();
      break;
    case "mark_notifications_read":
      kept_notifications = kept_notifications.map((kept) => ({ ...kept, is_read: true }));
      show_notifications();
      break;
    case "vibrate":
      buzz(effect.milliseconds as number);
      break;
    case "vibrate_pattern":
      buzz(400, effect.pattern === "alarm");
      break;
    case "vibrate_sequence":
      buzz((effect.steps_in_milliseconds as number[]).reduce((sum, step) => sum + step, 0));
      break;
    case "stop_vibrating":
      if (vibration_timer) clearTimeout(vibration_timer);
      is_vibrating.value = false;
      break;
    case "start_heart_rate":
      is_heart_rate_running.value = true;
      heart_rate_status.value = 3;
      deliver("heart_rate_reading", { beats_per_minute: 60 + Math.floor(Math.random() * 30), confidence: 90 });
      break;
    case "measure_heart_rate":
      deliver("heart_rate_reading", { beats_per_minute: 60 + Math.floor(Math.random() * 30), confidence: 90 });
      break;
    case "stop_heart_rate":
      is_heart_rate_running.value = false;
      heart_rate_status.value = 0;
      break;
    case "set_bluetooth":
      change_setting("bluetooth", effect.on ? 1 : 0);
      break;
    case "set_app_enabled":
      packages.value = packages
        .peek()
        .map((app) => (app.id === effect.id && app.refusal === 0 ? { ...app, enabled: effect.on as boolean } : app));
      break;
    case "step_watch_face": {
      const faces = packages.peek().filter((app) => app.is_face && app.enabled);
      const shown = faces.findIndex((face) => face.name === watch_face_name.peek());
      const next = faces[(((shown + (effect.steps as number)) % faces.length) + faces.length) % faces.length];
      if (next) watch_face_name.value = next.name;
      break;
    }
    case "reset_step_trip":
      step_history.value = { ...step_history.peek(), trip: 0 };
      break;
    case "set_setting":
      change_setting(effect.key as string, effect.value as number);
      break;
    case "set_do_not_disturb":
      change_setting("do_not_disturb", effect.on ? 1 : 0);
      break;
  }
}

// ============================================================================
// Notifications
// ============================================================================

/** As many as the watch keeps. */
const MOST_KEPT_NOTIFICATIONS = 5;

let kept_notifications: { notification: KeptNotification; is_read: boolean }[] = [];
let next_app_notification_id = -1;

function show_notifications() {
  batch(() => {
    notifications.value = kept_notifications.map((kept) => kept.notification);
    unread_notifications.value = kept_notifications.filter((kept) => !kept.is_read).length;
  });
}

/** Keeps a notification as the newest and unread, in place of any with its id, as the watch does. */
export function keep_notification(arrived: Omit<KeptNotification, "icon">) {
  const notification = { ...arrived, icon: notification_icon(arrived.app, arrived.body) };
  const others = kept_notifications.filter((kept) => kept.notification.id !== notification.id);
  kept_notifications = [{ notification, is_read: false }, ...others].slice(0, MOST_KEPT_NOTIFICATIONS);
  show_notifications();
}

/** Forgets every kept notification, for tests. */
export function clear_notifications() {
  kept_notifications = [];
  next_app_notification_id = -1;
  show_notifications();
}

/** Changes a setting the way the watch does, then tells subscribed apps; unknown names are ignored. */
function change_setting(key: string, value: number) {
  if (key === "brightness") brightness.value = value;
  else if (key === "do_not_disturb") is_do_not_disturb.value = value !== 0;
  else if (key === "bluetooth") is_bluetooth_enabled.value = value !== 0;
  else if (key in settings) {
    const setting = settings[key as keyof typeof settings];
    setting.value = (typeof setting.peek() === "boolean" ? value !== 0 : value) as never;
  } else return;
  // Sleep mode is the third state of do not disturb, as the watch's settings keep it.
  if (key === "sleep_mode" && value !== 0) is_do_not_disturb.value = true;
  if (key === "do_not_disturb" && value === 0) settings.sleep_mode.value = false;
  deliver("settings_change", { key });
}

// ============================================================================
// Side button
// ============================================================================

/** The side button: back in an app, and on the face the screen turns off or on. */
export function press_button() {
  const app = open_app.peek();
  if (app && app.kind !== "face") {
    emit({ kind: "back" });
  } else {
    is_screen_on.value = !is_screen_on.peek();
  }
}

// ============================================================================
// Step history
// ============================================================================

const HOURS_PER_DAY = 24;

/** Adds new steps to today, the hour they were taken in and the trip, as the watch does with each reading. */
function add_steps(steps: number) {
  const history = step_history.peek();
  const hour = hour_of(now_milliseconds.peek());
  step_history.value = {
    trip: history.trip + steps,
    days: history.days.map((day, days_back) => (days_back === 0 ? { total: day.total + steps } : day)),
    hours: history.hours.map((entry, place) => (place === hour ? { steps: entry.steps + steps } : entry)),
  };
}

/** Moves every day one back at midnight and starts today from zero, as the watch does. */
function start_new_step_day() {
  const history = step_history.peek();
  step_history.value = {
    trip: history.trip,
    days: [{ total: 0 }, ...history.days.slice(0, STEP_HISTORY_DAYS - 1)],
    hours: [
      ...Array.from({ length: HOURS_PER_DAY }, () => ({ steps: 0 })),
      ...history.hours.slice(0, (STEP_HISTORY_DAYS - 1) * HOURS_PER_DAY),
    ],
  };
  steps_today.value = 0;
}

/** Keeps `step_history` in step with `steps_today` and the clock; returns a function that stops it. */
function keep_step_history(): () => void {
  let counted_steps = steps_today.peek();
  return effect(() => {
    const steps = steps_today.value;
    if (steps > counted_steps) add_steps(steps - counted_steps);
    counted_steps = steps;
  });
}

// ============================================================================
// Subscriptions
// ============================================================================

interface Scheduled {
  app: App;
  subscription: Subscription;
  next_due: Instant | null;
}

let scheduled: Scheduled[] = [];

function next_daily(after: Instant, time_of_day: TimeOfDay): Instant {
  const offset = (time_of_day.hour * 60 + time_of_day.minute) * MINUTE;
  const today = start_of_day(after) + offset;
  return today > after ? today : start_of_day(after + DAY) + offset;
}

function time_of_day_of(subscription: Extract<Subscription, { kind: "daily" }>): TimeOfDay | null {
  const value = subscription.time_of_day;
  return "hour" in value ? value : value.value;
}

/** Whether a subscription may run now: short periods and `only_while_open` ones only while its app is open. */
function may_run(entry: Scheduled): boolean {
  const subscription = entry.subscription;
  if (subscription.only_while_open) return open_app.peek() === entry.app;
  if (subscription.kind === "every" && period_in_milliseconds(subscription.period) < MINUTE) {
    return open_app.peek() === entry.app;
  }
  return true;
}

function crosses(previous: Instant, current: Instant, unit: "minute" | "hour" | "day"): boolean {
  if (unit === "day") return start_of_day(previous) !== start_of_day(current);
  const size = unit === "minute" ? MINUTE : HOUR;
  return Math.floor(previous / size) !== Math.floor(current / size);
}

function check_time_subscriptions(previous: Instant, current: Instant) {
  for (const entry of scheduled) {
    const subscription = entry.subscription;
    if (!may_run(entry)) continue;
    switch (subscription.kind) {
      case "every": {
        const period = period_in_milliseconds(subscription.period);
        entry.next_due ??= current + period;
        if (current >= entry.next_due) {
          entry.next_due = current + period;
          run_app_handler(entry.app, subscription.handler, { time: current });
        }
        break;
      }
      case "at": {
        const when = subscription.when.peek();
        if (when !== null && when > previous && when <= current) {
          run_app_handler(entry.app, subscription.handler, { time: current });
        }
        break;
      }
      case "daily": {
        const time_of_day = time_of_day_of(subscription);
        if (time_of_day && next_daily(previous, time_of_day) <= current) {
          run_app_handler(entry.app, subscription.handler, { time: current });
        }
        break;
      }
      case "clock_change":
        if (crosses(previous, current, subscription.unit)) {
          run_app_handler(entry.app, subscription.handler, { time: current });
        }
        break;
    }
  }
}

/** Starts the clock and the scheduler for every installed app; returns a function that stops it. */
export function start_scheduler(): () => void {
  scheduled = installed_apps.flatMap((app) =>
    app.subscriptions.map((subscription) => ({ app, subscription, next_due: null })),
  );
  let previous = now_milliseconds.peek();
  const stop_counting_steps = keep_step_history();
  const stop_watching = effect(() => {
    const current = now_milliseconds.value;
    if (crosses(previous, current, "day")) start_new_step_day();
    if (current !== previous) check_time_subscriptions(previous, current);
    previous = current;
  });
  const stop_listening = listen_for_effects(carry_out);
  start_clock();
  return () => {
    stop_counting_steps();
    stop_watching();
    stop_listening();
  };
}

/** Delivers an event to every installed app subscribed to it, as when the phone sends one. */
export function deliver(kind: Subscription["kind"], event: object, filter?: (subscription: Subscription) => boolean) {
  if (kind === "heart_rate_reading") {
    const reading = event as { beats_per_minute: number };
    heart_rate.value = reading.beats_per_minute;
    heart_rate_measured_at.value = now_milliseconds.peek();
  }
  if (kind === "weather") {
    const forecast = event as {
      temperature: number;
      condition: never;
      low: number;
      high: number;
      location: string;
      forecast?: WeatherDay[];
    };
    current_weather.value = {
      temperature: forecast.temperature,
      condition: forecast.condition,
      low: forecast.low,
      high: forecast.high,
      location: forecast.location,
      received_at: now_milliseconds.peek(),
      sunrise: null,
      sunset: null,
      forecast: forecast.forecast ?? [],
    };
  }
  if (kind === "notification") {
    const notification = event as Omit<KeptNotification, "time" | "icon"> & { time?: Instant };
    keep_notification({ ...notification, time: notification.time ?? now_milliseconds.peek() });
  }
  if (kind === "call") {
    const call = event as { kind: "incoming" | "answered" | "ended"; caller: string };
    current_call.value = { caller: call.caller, state: call.kind };
  }
  if (kind === "music") current_music.value = { ...(event as typeof current_music.value) };
  if (kind === "navigation") current_navigation.value = { ...(event as typeof current_navigation.value) };
  for (const entry of scheduled) {
    if (entry.subscription.kind !== kind) continue;
    if (filter && !filter(entry.subscription)) continue;
    if (!may_run(entry)) continue;
    run_app_handler(entry.app, entry.subscription.handler as (event: object) => void, event);
  }
}

// ============================================================================
// Clock control
// ============================================================================

/** Stops the clock at an instant, for tests and screenshots. */
export function freeze_clock(at: Instant) {
  clock.frozen_at = at;
  now_milliseconds.value = at;
}

/** Moves a frozen clock forward, in steps no longer than a second so every tick is seen. */
export function advance_clock(milliseconds: number) {
  const target = (clock.frozen_at ?? now_milliseconds.peek()) + milliseconds;
  let current = clock.frozen_at ?? now_milliseconds.peek();
  while (current < target) {
    current = Math.min(target, current + 1000);
    clock.frozen_at = current;
    now_milliseconds.value = current;
  }
}

export function unfreeze_clock() {
  clock.frozen_at = null;
}

// ============================================================================
// Showing the open app
// ============================================================================

/** The open app's current screen, or nothing. */
export function OpenScreen() {
  const app = open_app.value;
  const name = screen_stack.value[screen_stack.value.length - 1];
  if (!app) return null;
  const ScreenComponent = app.screens[name] ?? app.screens.main;
  return <ScreenComponent key={`${app.id}:${name}`} />;
}
