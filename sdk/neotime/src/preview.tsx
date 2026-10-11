// The browser preview's stand-in for the runtime: it shows an app's screens, carries out its effects
// and runs its subscriptions from the clock. Apps never import this module.

import { batch, effect, signal } from "@preact/signals";
import { clock, emit, listen_for_effects, now_milliseconds, start_clock, type Effect } from "./host.ts";
import {
  current_screen,
  heart_rate,
  heart_rate_measured_at,
  period_in_milliseconds,
  start_of_day,
  steps_today,
  DAY,
  HOUR,
  MINUTE,
  type App,
  type Instant,
  type Subscription,
  type TimeOfDay,
} from "./index.ts";
import { active_screen, run_handler, swipe_fallback, type ScreenHandlers } from "./widgets.tsx";

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
      show_toast(`${effect.title}: ${effect.body}`);
      buzz(200);
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
    case "measure_heart_rate":
    case "start_heart_rate":
      deliver("heart_rate_reading", { beats_per_minute: 60 + Math.floor(Math.random() * 30), confidence: 90 });
      break;
    case "reset_steps":
      steps_today.value = 0;
      break;
  }
}

// ============================================================================
// Side button and system swipes
// ============================================================================

/** The side button: the screen's handler if it has one, otherwise back. */
export function press_button(kind: "press" | "long_press" = "press") {
  const app = open_app.peek();
  const handlers: ScreenHandlers = active_screen;
  if (app && app.kind !== "face" && handlers.onButton) {
    run_app_handler(app, handlers.onButton, { kind });
  } else if (app && app.kind !== "face") {
    emit({ kind: "back" });
  } else {
    is_screen_on.value = !is_screen_on.peek();
  }
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
  const stop_watching = effect(() => {
    const current = now_milliseconds.value;
    if (current !== previous) check_time_subscriptions(previous, current);
    previous = current;
  });
  const stop_listening = listen_for_effects(carry_out);
  swipe_fallback.handle = (direction) => {
    if (direction === "right") emit({ kind: "back" });
  };
  start_clock();
  return () => {
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
