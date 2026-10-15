import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  brightness,
  is_bluetooth_enabled,
  is_do_not_disturb,
  set_setting,
  settings,
  shake_speed,
  watch_face_name,
} from "battletime";
import {
  effect_log,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  press_button,
  screen_stack,
  start_scheduler,
  swipe,
  unfreeze_clock,
} from "battletime/preview";
import app, {
  calibrate,
  calibration_stage,
  chosen_day,
  chosen_minute_of_day,
  chosen_month,
  chosen_month_length,
  chosen_shake_threshold,
  chosen_year,
  next_calibration_stage,
  next_shake_sample_at,
  sample_shake,
  save_date_time,
  set_brightness_level,
  set_shake_threshold,
  set_timeout,
  toggle_24_hour_clock,
  toggle_alarms_in_do_not_disturb,
  toggle_bluetooth,
  toggle_always_on_screen,
  toggle_do_not_disturb,
  toggle_lower_to_sleep,
  toggle_metric_units,
  toggle_raise_to_wake,
  toggle_shake_to_wake,
  toggle_sleep_mode,
} from "./app.tsx";

let stop_scheduler = () => {};

function widgets(container: Element, kind: string): HTMLElement[] {
  return [
    ...container.querySelectorAll(`[data-widget=${kind}]`),
  ] as HTMLElement[];
}

function tap(element: HTMLElement) {
  act(() => element.click());
}

/** Taps the row whose first label reads `title`. */
function open_row(container: Element, title: string) {
  const row = widgets(container, "row").find((candidate) =>
    candidate.textContent?.startsWith(title),
  );
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
    set_setting("heart_rate_interval", 0);
    set_setting("sleep_mode", 0);
    set_setting("raise_to_wake", 0);
    set_setting("lower_to_sleep", 0);
    set_setting("shake_to_wake", 0);
    set_setting("shake_threshold", 150);
    shake_speed.value = 0;
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

  it("turns Bluetooth and alarms in do not disturb off and on again", () => {
    toggle_bluetooth();
    toggle_alarms_in_do_not_disturb();

    expect(is_bluetooth_enabled.value).toBe(false);
    expect(settings.alarms_in_do_not_disturb.value).toBe(false);

    toggle_bluetooth();
    toggle_alarms_in_do_not_disturb();

    expect(is_bluetooth_enabled.value).toBe(true);
    expect(settings.alarms_in_do_not_disturb.value).toBe(true);
  });

  it("picks a preset theme by number", () => {
    set_setting("theme", 2);
    expect(settings.theme.value).toBe(2);
  });

  it("keeps the step goal between 1,000 and 50,000", () => {
    const { container } = render(<OpenScreen />);
    open_row(container, "Step goal");
    const button = (place: number) =>
      widgets(container, "stepper")[0].querySelectorAll("button")[
        place
      ] as HTMLElement;
    act(() => {
      settings.step_goal.value = 1000;
    });
    tap(button(0));
    expect(settings.step_goal.value).toBe(1000);

    act(() => {
      settings.step_goal.value = 49800;
    });
    tap(button(1));
    expect(settings.step_goal.value).toBe(50000);
  });

  it("turns every switch in the list", () => {
    const { container } = render(<OpenScreen />);

    for (const toggle of widgets(container, "switch")) tap(toggle);

    expect(settings.always_on_screen.value).toBe(true);
    expect(settings.uses_24_hour_clock.value).toBe(false);
    expect(settings.uses_metric_units.value).toBe(false);
    expect(is_do_not_disturb.value).toBe(true);
    expect(settings.sleep_mode.value).toBe(true);
    expect(settings.alarms_in_do_not_disturb.value).toBe(false);
    expect(is_bluetooth_enabled.value).toBe(false);
  });

  it("opens each screen of choices from its row and goes back with the button", () => {
    const { container } = render(<OpenScreen />);
    const screens: [string, string][] = [
      ["Brightness", "brightness"],
      ["Screen off", "timeout"],
      ["Step goal", "step_goal"],
      ["Chimes", "chimes"],
      ["Heart rate", "heart_rate_interval"],
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

  it("opens the face chooser from the Face row, which names the face shown", () => {
    watch_face_name.value = "Analog";
    const { container } = render(<OpenScreen />);

    open_row(container, "Face");

    expect(effect_log.value.at(-1)).toEqual({ kind: "choose_watch_face" });
    expect(screen_stack.value.at(-1)).toBe("main");
    expect(container.textContent).toContain("Analog");
    watch_face_name.value = "Digital";
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
    open_row(container, "Touch");
    widgets(container, "button").forEach(tap);
    act(() => press_button());
    act(() => press_button());
    open_row(container, "Heart rate");
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
    expect(settings.heart_rate_interval.value).toBe(6);
    expect(settings.step_goal.value).toBe(10500);
  });

  it("steps updates through off, on and until the next restart", () => {
    const { container } = render(<OpenScreen />);

    open_row(container, "Updates");
    expect(settings.allows_updates.value).toBe(2);
    open_row(container, "Updates");
    expect(settings.allows_updates.value).toBe(0);
    open_row(container, "Updates");
    expect(settings.allows_updates.value).toBe(1);
  });

  it("turns each wake gesture on from the wake screen", () => {
    const { container } = render(<OpenScreen />);
    open_row(container, "Wake");

    for (const toggle of widgets(container, "switch")) tap(toggle);

    expect(settings.raise_to_wake.value).toBe(true);
    expect(settings.shake_to_wake.value).toBe(true);
    expect(settings.lower_to_sleep.value).toBe(true);
  });

  it("opens the touch choices and the shake calibration from the wake screen", () => {
    const { container } = render(<OpenScreen />);
    open_row(container, "Wake");

    open_row(container, "Touch");
    expect(screen_stack.value.at(-1)).toBe("wake_touch");
    act(() => press_button());
    open_row(container, "Shake strength");
    expect(screen_stack.value.at(-1)).toBe("shake_threshold");
    expect(chosen_shake_threshold.value).toBe(150);
    expect(container.textContent).toContain("Calibrate");
  });

  it("sets the shake threshold from the arc", () => {
    set_shake_threshold(900);

    expect(chosen_shake_threshold.value).toBe(900);
    expect(settings.shake_threshold.value).toBe(900);
  });

  it("records a shake as the threshold, just below its peak", () => {
    calibrate();
    expect(calibration_stage.value).toBe(1);
    expect(chosen_shake_threshold.value).toBe(0);

    next_calibration_stage();
    expect(calibration_stage.value).toBe(2);
    shake_speed.value = 1500;
    sample_shake();
    shake_speed.value = 900;
    sample_shake();
    expect(chosen_shake_threshold.value).toBe(1200);
    expect(next_shake_sample_at.value).not.toBeNull();

    next_calibration_stage();
    expect(calibration_stage.value).toBe(0);
    expect(next_shake_sample_at.value).toBeNull();
    expect(settings.shake_threshold.value).toBe(1200);
    sample_shake();
    expect(chosen_shake_threshold.value).toBe(1200);
  });

  it("stops recording a shake when the button is tapped again", () => {
    calibrate();
    next_calibration_stage();
    shake_speed.value = 700;
    sample_shake();

    calibrate();

    expect(calibration_stage.value).toBe(0);
    expect(settings.shake_threshold.value).toBe(400);
  });

  it("opens the date and time pages on the local time now", () => {
    freeze_clock(Date.UTC(2026, 3, 15, 9, 30));
    const { container } = render(<OpenScreen />);
    open_row(container, "Date & time");
    unfreeze_clock();

    expect(screen_stack.value.at(-1)).toBe("clock");
    expect([chosen_day.value, chosen_month.value, chosen_year.value]).toEqual([
      15, 4, 2026,
    ]);
    expect(chosen_minute_of_day.value).toBe(9 * 60 + 30);
    expect(widgets(container, "roller").length).toBeGreaterThanOrEqual(3);
  });

  it("knows how long each month is, February from the year", () => {
    const lengths = (year: number) =>
      Array.from({ length: 12 }, (_, month) => {
        chosen_year.value = year;
        chosen_month.value = month + 1;
        return chosen_month_length.value;
      });
    expect(lengths(2026)).toEqual([
      31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ]);
    expect(lengths(2028)[1]).toBe(29);
  });

  it("sets the clock to the chosen day of the year and minute, then goes back", () => {
    const { container } = render(<OpenScreen />);
    open_row(container, "Date & time");
    chosen_year.value = 2028;
    chosen_month.value = 3;
    chosen_day.value = 1;
    chosen_minute_of_day.value = 23 * 60 + 59;

    save_date_time();

    expect(
      effect_log.value.filter((effect) => effect.kind === "set_clock").at(-1),
    ).toMatchObject({ year: 2028, day_of_year: 61, minute_of_day: 1439 });
    expect(screen_stack.value.at(-1)).toBe("main");
  });

  it("sets a day past the month's end as its last day", () => {
    chosen_year.value = 2026;
    chosen_month.value = 4;
    chosen_day.value = 31;
    chosen_minute_of_day.value = 0;

    save_date_time();

    expect(
      effect_log.value.filter((effect) => effect.kind === "set_clock").at(-1),
    ).toMatchObject({ year: 2026, day_of_year: 120 });
  });

  it("turns the day, month, year and hour rollers", () => {
    freeze_clock(Date.UTC(2026, 3, 15, 9, 30));
    const { container } = render(<OpenScreen />);
    open_row(container, "Date & time");
    unfreeze_clock();
    const step = (place: number) =>
      act(() =>
        (
          widgets(container, "roller")[place].querySelector(
            "[data-roller-offset='1']",
          ) as HTMLElement
        ).click(),
      );

    for (let place = 0; place < 3; place++) step(place);
    act(() => swipe("left"));
    step(0);

    expect([chosen_day.value, chosen_month.value, chosen_year.value]).toEqual([
      16, 5, 2027,
    ]);
    expect(chosen_minute_of_day.value).toBe(10 * 60 + 30);
  });

  it("turns each switch off and back on", () => {
    const switches = [
      [toggle_24_hour_clock, settings.uses_24_hour_clock],
      [toggle_metric_units, settings.uses_metric_units],
      [toggle_always_on_screen, settings.always_on_screen],
      [toggle_sleep_mode, settings.sleep_mode],
      [toggle_raise_to_wake, settings.raise_to_wake],
      [toggle_lower_to_sleep, settings.lower_to_sleep],
      [toggle_shake_to_wake, settings.shake_to_wake],
    ] as const;

    for (const [toggle, value] of switches) {
      const before = value.value;
      toggle();
      expect(value.value).toBe(!before);
      toggle();
      expect(value.value).toBe(before);
    }
  });

  it("names a theme of the wearer's own colours", () => {
    set_setting("theme", -1);
    const { container } = render(<OpenScreen />);
    expect(container.textContent).toContain("Own");
  });
});
