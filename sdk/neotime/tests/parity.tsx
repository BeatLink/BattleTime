// Compares the texts the browser preview shows with the watch's snapshots in harness/snapshots/texts.
// harness/tests/parity.rs writes those files from the real bytecode; docs/testing.md explains both sides.

import { act, cleanup, fireEvent, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  battery_percent,
  brightness,
  current_music,
  current_navigation,
  current_weather,
  heart_rate,
  heart_rate_measured_at,
  heart_rate_status,
  installed_apps,
  is_bluetooth_enabled,
  is_charging,
  is_do_not_disturb,
  is_heart_rate_running,
  is_phone_connected,
  next_alarm,
  reset_persisted,
  settings,
  steps_today,
  unread_notifications,
  utc_offset,
  HOUR,
  MINUTE,
  type App,
} from "neotime";
import {
  clear_storage,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  screen_stack,
  start_scheduler,
  swipe,
} from "neotime/preview";

// ============================================================================
// Snapshots
// ============================================================================

export interface ScreenSnapshot {
  name: string;
  /** The gesture on the main screen that reaches it: `tap "Text"`, `tap middle`, `long press middle` or `swipe up`. */
  reached_by: string | null;
  texts: string[];
}

interface Snapshot {
  screens: ScreenSnapshot[];
}

const snapshot_files = import.meta.glob<Snapshot>("../../../harness/snapshots/texts/*.json", {
  eager: true,
  import: "default",
});

/** The watch's snapshot for an app or face, by its name such as `app-stopwatch`. */
export function snapshot_named(name: string): Snapshot | undefined {
  return snapshot_files[`../../../harness/snapshots/texts/${name}.json`];
}

export function snapshot_names(kind: "app" | "face"): string[] {
  return Object.keys(snapshot_files)
    .map((path) => path.replace(/^.*\//, "").replace(/\.json$/, ""))
    .filter((name) => name.startsWith(`${kind}-`));
}

// ============================================================================
// Known differences
// ============================================================================

export interface KnownDifference {
  snapshot: string;
  screen: string;
  /** Shown only on the watch, or only in the preview. */
  side: "watch_only" | "preview_only";
  text: string;
  reason: string;
}

/** Real, accepted differences between the watch and the preview; the test fails once one goes, so this only shrinks. */
export const KNOWN_DIFFERENCES: KnownDifference[] = [
  { snapshot: "app-dice", screen: "main", side: "watch_only", text: "1", reason: "The watch rolls with its own random generator, not the browser's." },
  { snapshot: "app-dice", screen: "main", side: "preview_only", text: "4", reason: "The watch rolls with its own random generator, not the browser's." },
  { snapshot: "app-heart", screen: "main", side: "watch_only", text: "---", reason: "The watch waits for the sensor; the preview hands back a reading at once." },
  { snapshot: "app-heart", screen: "main", side: "watch_only", text: "Starting...", reason: "The watch waits for the sensor; the preview hands back a reading at once." },
  { snapshot: "app-heart", screen: "main", side: "preview_only", text: "075", reason: "The watch waits for the sensor; the preview hands back a reading at once." },
  { snapshot: "app-heart", screen: "main", side: "preview_only", text: "Measuring...", reason: "The watch waits for the sensor; the preview hands back a reading at once." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "10000", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Chimes", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Dark", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Do not disturb", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Metric units", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Off", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Step goal", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "app-settings", screen: "main", side: "preview_only", text: "Theme", reason: "These rows sit below the screen on the watch, and the preview test has no layout to tell." },
  { snapshot: "face-terminal", screen: "main", side: "preview_only", text: "user@watch:~ $", reason: "The last line falls below the screen on the watch, and the preview test has no layout to tell." },
];

// ============================================================================
// The fake watch's starting values
// ============================================================================

/** Monday 5 January 2026, 09:00:00 UTC, the harness's START_TIME_IN_MILLISECONDS. */
export const START_TIME_IN_MILLISECONDS = 1_767_603_600_000;

/** Sets every system value to what harness/src/fake_system.rs starts with. */
export function set_fake_system_values() {
  const now = START_TIME_IN_MILLISECONDS;
  utc_offset.value = 0;
  battery_percent.value = 87;
  is_charging.value = false;
  steps_today.value = 4321;
  heart_rate.value = 64;
  heart_rate_measured_at.value = now - 5 * MINUTE;
  is_phone_connected.value = true;
  unread_notifications.value = 0;
  is_do_not_disturb.value = false;
  brightness.value = 2;
  settings.uses_24_hour_clock.value = true;
  settings.uses_metric_units.value = true;
  settings.step_goal.value = 10_000;
  settings.screen_timeout_seconds.value = 0;
  settings.always_on_screen.value = false;
  settings.chimes.value = 0;
  settings.theme.value = 0;
  is_heart_rate_running.value = false;
  heart_rate_status.value = 0;
  is_bluetooth_enabled.value = true;
  next_alarm.value = null;
  current_weather.value = {
    temperature: 18,
    condition: "clear",
    low: 11,
    high: 21,
    location: "Test City",
    received_at: now - 20 * MINUTE,
    sunrise: now - 2 * HOUR,
    sunset: now + 8 * HOUR,
    forecast: [
      { condition: "few_clouds", low: 9, high: 17 },
      { condition: "heavy_clouds", low: 7, high: 14 },
      { condition: "clear", low: 10, high: 20 },
    ],
  };
  current_music.value = { artist: "The Testers", track: "Test Pattern", is_playing: true, position: 72_000, length: 215_000 };
  current_navigation.value = {
    turn: "straight",
    instruction: "Turn left onto Main Street",
    distance: 350,
    progress_percent: 40,
  };
  installed_apps.value = [];
}

// ============================================================================
// Reading the preview
// ============================================================================

/** Widgets whose text the watch does not count as text, such as an icon's symbol. */
const WIDGETS_WITHOUT_TEXT = new Set(["icon"]);

/** The watch draws a button matrix's keys inside the one widget, so the test watch reads no text from them. */
function is_button_matrix_key(element: Element): boolean {
  return element.parentElement?.closest("[data-widget=button-matrix]") != null;
}

/** The text a widget holds itself, leaving out the widgets inside it. */
function own_text(element: Element): string {
  return [...element.childNodes]
    .filter((node) => node.nodeType === Node.TEXT_NODE)
    .map((node) => node.textContent ?? "")
    .join("");
}

/** The text of every widget the preview shows, in page order. */
export function shown_texts(root: Element): string[] {
  return [...root.querySelectorAll("[data-widget]")]
    .filter((element) => !WIDGETS_WITHOUT_TEXT.has(element.getAttribute("data-widget") ?? ""))
    .filter((element) => !is_button_matrix_key(element))
    .map(own_text)
    .filter((text) => text !== "");
}

/** The widget that holds exactly this text itself, as the watch's tap_text finds it. */
function widget_with_text(root: Element, text: string): Element | undefined {
  return [...root.querySelectorAll("[data-widget]")].find((element) => own_text(element) === text);
}

const LONG_PRESS_IN_MILLISECONDS = 500;

/** Makes a gesture the watch snapshot names, on the preview's open screen. */
function make_gesture(root: Element, gesture: string) {
  const screen = root.querySelector("[data-widget=screen]") ?? root;
  if (gesture.startsWith("tap \"")) {
    const text = JSON.parse(gesture.slice("tap ".length)) as string;
    const widget = widget_with_text(root, text);
    if (!widget) throw new Error(`no widget reads ${JSON.stringify(text)}`);
    act(() => {
      fireEvent.click(widget);
    });
  } else if (gesture === "tap middle") {
    act(() => {
      fireEvent.click(screen);
    });
  } else if (gesture === "long press middle") {
    act(() => {
      fireEvent.pointerDown(screen);
    });
    act(() => {
      vi.advanceTimersByTime(LONG_PRESS_IN_MILLISECONDS);
    });
    act(() => {
      fireEvent.pointerUp(screen);
    });
  } else if (gesture.startsWith("swipe ")) {
    act(() => swipe(gesture.slice("swipe ".length) as "up" | "down" | "left" | "right"));
  } else {
    throw new Error(`the preview cannot make the gesture ${gesture}`);
  }
}

/** Opens an app at the fixed clock, makes the gesture if any, and gives the screen it shows and its texts. */
export function preview_screen(app: App, gesture: string | null): { name: string; texts: string[] } {
  const { container } = render(<OpenScreen />);
  act(() => open(app));
  if (gesture) make_gesture(container, gesture);
  const stack = screen_stack.peek();
  return { name: stack[stack.length - 1], texts: shown_texts(container) };
}

// ============================================================================
// Comparing
// ============================================================================

/** The texts only one side shows, counting repeats, each list sorted. */
export function text_differences(watch: string[], preview: string[]): { watch_only: string[]; preview_only: string[] } {
  const remaining = [...preview];
  const watch_only: string[] = [];
  for (const text of watch) {
    const index = remaining.indexOf(text);
    if (index >= 0) remaining.splice(index, 1);
    else watch_only.push(text);
  }
  return { watch_only: watch_only.sort(), preview_only: remaining.sort() };
}

function known_for(snapshot: string, screen: string, side: KnownDifference["side"]): string[] {
  return KNOWN_DIFFERENCES.filter(
    (difference) => difference.snapshot === snapshot && difference.screen === screen && difference.side === side,
  )
    .map((difference) => difference.text)
    .sort();
}

/** Checks every app or face's screens in the preview against the watch's snapshots. */
export function describe_parity(kind: "app" | "face", modules: Record<string, { default: App }>) {
  const apps = Object.values(modules).map((module) => module.default);
  let stop_scheduler = () => {};

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(START_TIME_IN_MILLISECONDS);
    // The watch's random generator is not the browser's, so the preview's is pinned to keep each run the same.
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    freeze_clock(START_TIME_IN_MILLISECONDS);
    clear_storage();
    set_fake_system_values();
  });

  afterEach(() => {
    cleanup();
    act(() => open(null));
    stop_scheduler();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it(`has a watch snapshot for every ${kind} and a ${kind} for every snapshot`, () => {
    const ids = apps.map((app) => `${kind}-${app.id}`).sort();
    expect(snapshot_names(kind).sort()).toEqual(ids);
  });

  describe.each(apps.map((app) => [app.id, app] as const))("%s", (id, app) => {
    const name = `${kind}-${id}`;
    const screens = snapshot_named(name)?.screens ?? [];

    it.each(screens.map((screen) => [screen.name, screen] as const))(
      "shows the watch's texts on its %s screen",
      (_screen_name, watch_screen) => {
        reset_persisted(app);
        install_apps([app]);
        stop_scheduler = start_scheduler();

        const shown = preview_screen(app, watch_screen.reached_by);

        expect(shown.name, `${watch_screen.reached_by} should reach ${watch_screen.name}`).toBe(watch_screen.name);
        const differences = text_differences(watch_screen.texts, shown.texts);
        expect({ name, screen: watch_screen.name, ...differences }).toEqual({
          name,
          screen: watch_screen.name,
          watch_only: known_for(name, watch_screen.name, "watch_only"),
          preview_only: known_for(name, watch_screen.name, "preview_only"),
        });
      },
    );
  });

  it("lists no known difference for a screen the watch has no snapshot of", () => {
    const listed = KNOWN_DIFFERENCES.filter((difference) => difference.snapshot.startsWith(`${kind}-`));
    const stale = listed.filter(
      (difference) => !snapshot_named(difference.snapshot)?.screens.some((screen) => screen.name === difference.screen),
    );
    expect(stale).toEqual([]);
  });
}
