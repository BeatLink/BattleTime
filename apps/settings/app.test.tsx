import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { brightness, is_bluetooth_enabled, is_do_not_disturb, set_setting, settings } from "neotime";
import { install_apps, open, OpenScreen, press_button, screen_stack, start_scheduler } from "neotime/preview";
import app, {
  change_step_goal,
  set_brightness_level,
  set_timeout,
  toggle_24_hour_clock,
  toggle_alarms_in_do_not_disturb,
  toggle_bluetooth,
  toggle_do_not_disturb,
  toggle_updates,
} from "./app.tsx";

let stop_scheduler = () => {};

function widgets(container: Element, kind: string): HTMLElement[] {
  return [...container.querySelectorAll(`[data-widget=${kind}]`)] as HTMLElement[];
}

function tap(element: HTMLElement) {
  act(() => element.click());
}

/** Taps the row whose first label reads `title`. */
function open_row(container: Element, title: string) {
  const row = widgets(container, "row").find((candidate) => candidate.textContent?.startsWith(title));
  tap(row as HTMLElement);
}

describe("settings", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    set_setting("brightness", 2);
    set_setting("screen_timeout_seconds", 15);
    set_setting("uses_24_hour_clock", 1);
    set_setting("chimes", 0);
    set_setting("step_goal", 10000);
    set_setting("always_on_screen", 0);
    set_setting("uses_metric_units", 1);
    set_setting("theme", 0);
    set_setting("do_not_disturb", 0);
    set_setting("bluetooth", 1);
    set_setting("allows_updates", 1);
    set_setting("wake_on_touch", 1);
    set_setting("alarms_in_do_not_disturb", 1);
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("changes the watch's settings", () => {
    set_brightness_level(3);
    set_timeout(30);
    toggle_24_hour_clock();
    toggle_do_not_disturb();

    expect(brightness.value).toBe(3);
    expect(settings.screen_timeout_seconds.value).toBe(30);
    expect(settings.uses_24_hour_clock.value).toBe(false);
    expect(is_do_not_disturb.value).toBe(true);
  });

  it("turns Bluetooth, updates and alarms in do not disturb off and on again", () => {
    toggle_bluetooth();
    toggle_updates();
    toggle_alarms_in_do_not_disturb();

    expect(is_bluetooth_enabled.value).toBe(false);
    expect(settings.allows_updates.value).toBe(false);
    expect(settings.alarms_in_do_not_disturb.value).toBe(false);

    toggle_bluetooth();
    toggle_updates();
    toggle_alarms_in_do_not_disturb();

    expect(is_bluetooth_enabled.value).toBe(true);
    expect(settings.allows_updates.value).toBe(true);
    expect(settings.alarms_in_do_not_disturb.value).toBe(true);
  });

  it("picks a preset theme by number", () => {
    set_setting("theme", 2);
    expect(settings.theme.value).toBe(2);
  });

  it("keeps the step goal between 1,000 and 50,000", () => {
    settings.step_goal.value = 1000;
    change_step_goal(-500);
    expect(settings.step_goal.value).toBe(1000);

    settings.step_goal.value = 49800;
    change_step_goal(500);
    expect(settings.step_goal.value).toBe(50000);
  });

  it("turns every switch in the list", () => {
    const { container } = render(<OpenScreen />);

    for (const toggle of widgets(container, "switch")) tap(toggle);

    expect(settings.always_on_screen.value).toBe(true);
    expect(settings.uses_24_hour_clock.value).toBe(false);
    expect(settings.uses_metric_units.value).toBe(false);
    expect(is_do_not_disturb.value).toBe(true);
    expect(settings.alarms_in_do_not_disturb.value).toBe(false);
    expect(is_bluetooth_enabled.value).toBe(false);
    expect(settings.allows_updates.value).toBe(false);
  });

  it("opens each screen of choices from its row and goes back with the button", () => {
    const { container } = render(<OpenScreen />);
    const screens: [string, string][] = [
      ["Brightness", "brightness"],
      ["Screen off", "timeout"],
      ["Step goal", "step_goal"],
      ["Chimes", "chimes"],
      ["Wake", "wake"],
      ["Theme", "theme"],
    ];

    for (const [title, screen] of screens) {
      open_row(container, title);
      expect(screen_stack.value.at(-1)).toBe(screen);
      act(() => press_button());
      expect(screen_stack.value.at(-1)).toBe("main");
    }
  });

  it("takes the last choice tapped on each screen", () => {
    const { container } = render(<OpenScreen />);

    open_row(container, "Brightness");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    open_row(container, "Screen off");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    open_row(container, "Chimes");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    open_row(container, "Theme");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    open_row(container, "Wake");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    open_row(container, "Step goal");
    const [fewer, more] = widgets(container, "button");
    tap(more);
    tap(more);
    tap(fewer);

    expect(brightness.value).toBe(3);
    expect(settings.screen_timeout_seconds.value).toBe(120);
    expect(settings.chimes.value).toBe(2);
    expect(settings.theme.value).toBe(5);
    expect(settings.wake_on_touch.value).toBe(2);
    expect(settings.step_goal.value).toBe(10500);
  });
});
