// Checks where the step gauge's needle points, which weather icon the sidebar shows by day and by night, and the settings screen.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  current_weather,
  is_charging,
  reset_persisted,
  settings,
  steps_today,
  unread_notifications,
  type CurrentWeather,
} from "neotime";
import { icon_symbol, icons } from "neotime/icons";
import { freeze_clock, install_apps, open, OpenScreen, screen_stack, start_scheduler, unfreeze_clock } from "neotime/preview";
import face, { step_gauge_value } from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

const NO_WEATHER: CurrentWeather = {
  temperature: 0,
  condition: "clear",
  low: 0,
  high: 0,
  location: "",
  received_at: null,
  sunrise: null,
  sunset: null,
  forecast: [],
};

/** An instant on the test day at an hour and minute. */
function instant_at(hour: number, minute = 0): number {
  return new Date(2026, 9, 11, hour, minute, 0).getTime();
}

/** The weather icon's symbol in the sidebar at an hour of the day. */
function weather_symbol_at(hour: number): string {
  freeze_clock(instant_at(hour));
  const Face = face.screens.main;
  const { container } = render(<Face />);
  const icon = [...container.querySelectorAll<HTMLElement>("[data-widget=icon]")].find(
    (element) => element.style.left === "206px",
  );
  return icon?.textContent ?? "";
}

// ============================================================================
// Tests
// ============================================================================

describe("pinetime style", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    reset_persisted(face);
    steps_today.value = 0;
    settings.step_goal.value = 10000;
    current_weather.value = NO_WEATHER;
  });

  it("points the full gauge by percent of the goal, wrapping past it as InfiniTime does", () => {
    steps_today.value = 2500;
    expect(step_gauge_value.value).toBe(25);
    steps_today.value = 12000;
    expect(step_gauge_value.value).toBe(20);
  });

  it("sweeps the half gauge's needle from nine o'clock over the top to three o'clock", () => {
    const Settings = face.screens.settings;
    render(<Settings />);
    fireEvent.click(screen.getByText("Steps: Full"));
    cleanup();
    const Face = face.screens.main;
    const needle_end = () => {
      const { container } = render(<Face />);
      const needle = [...container.querySelectorAll("[data-widget=scale] line")].at(-1)!;
      const end = { x: Number(needle.getAttribute("x2")), y: Number(needle.getAttribute("y2")) };
      cleanup();
      return end;
    };
    expect(needle_end().x).toBeLessThan(5);
    steps_today.value = 5000;
    expect(needle_end().y).toBeLessThan(5);
    steps_today.value = 9900;
    expect(needle_end().x).toBeGreaterThan(32);
  });

  it("shows a cross before any forecast, then the sun by day and the moon by night", () => {
    expect(weather_symbol_at(12)).toBe("✕");
    cleanup();
    current_weather.value = { ...NO_WEATHER, temperature: 20, received_at: 1 };
    expect(weather_symbol_at(12)).toBe("☀");
    cleanup();
    expect(weather_symbol_at(22)).toBe("☾");
  });

  it("takes night from the forecast's sunset and sunrise when the phone sends them", () => {
    current_weather.value = {
      ...NO_WEATHER,
      temperature: 20,
      received_at: 1,
      sunrise: instant_at(7, 30),
      sunset: instant_at(20, 15),
    };
    expect(weather_symbol_at(19)).toBe("☀");
    cleanup();
    expect(weather_symbol_at(21)).toBe("☾");
    cleanup();
    expect(weather_symbol_at(7)).toBe("☾");
  });
});

// ============================================================================
// Settings and sidebar
// ============================================================================

/** Every label's text on the face as drawn at an hour of the test day. */
function labels_at(hour: number, minute = 0, second = 0): string[] {
  freeze_clock(new Date(2026, 9, 11, hour, minute, second).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

function icon_symbols(): string[] {
  return [...document.querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent ?? "");
}

/** The colour names the settings screen shows for the time, the bar and the background. */
function chosen_colors(): string[] {
  return [...document.querySelectorAll("[data-widget=label]")]
    .filter((_label, place) => place % 2 === 1)
    .map((label) => label.textContent ?? "");
}

function press(button_place: number, times = 1) {
  for (let press = 0; press < times; press++) fireEvent.click(screen.getAllByRole("button")[button_place]);
}

const TIME_BACK = 0;
const TIME_ON = 1;
const BAR_BACK = 2;
const BACKGROUND_BACK = 4;
const BACKGROUND_ON = 5;

describe("pinetime style settings and sidebar", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    unfreeze_clock();
    reset_persisted(face);
    steps_today.value = 0;
    is_charging.value = false;
    unread_notifications.value = 0;
    settings.uses_24_hour_clock.value = true;
    settings.uses_metric_units.value = true;
    current_weather.value = NO_WEATHER;
  });

  it("starts teal on black and skips a time colour that matches the background", () => {
    const Settings = face.screens.settings;
    render(<Settings />);
    expect(chosen_colors()).toEqual(["Teal", "Teal", "Black"]);
    press(TIME_BACK, 7);
    expect(chosen_colors()[0]).toBe("Red");
    press(TIME_BACK);
    expect(chosen_colors()[0]).toBe("Gray");
    press(TIME_ON);
    expect(chosen_colors()[0]).toBe("Red");
  });

  it("never makes the bar black and skips a background that matches the time", () => {
    const Settings = face.screens.settings;
    render(<Settings />);
    press(BAR_BACK, 8);
    expect(chosen_colors()[1]).toBe("Gray");
    press(TIME_BACK, 7);
    press(BACKGROUND_ON);
    expect(chosen_colors()[2]).toBe("Maroon");
  });

  it("wraps the colours round at both ends", () => {
    const Settings = face.screens.settings;
    render(<Settings />);
    press(TIME_ON, 7);
    expect(chosen_colors()[0]).toBe("White");
    fireEvent.click(screen.getByText("Reset"));
    press(BACKGROUND_BACK, 4);
    expect(chosen_colors()[2]).toBe("Pink");
  });

  it("picks random colours that never clash, and resets to teal on black", () => {
    vi.spyOn(Math, "random").mockReturnValue(0.17);
    const Settings = face.screens.settings;
    render(<Settings />);
    fireEvent.click(screen.getByText("Random"));
    expect(chosen_colors()).toEqual(["Black", "Gray", "Red"]);
    fireEvent.click(screen.getByText("Reset"));
    expect(chosen_colors()).toEqual(["Teal", "Teal", "Black"]);
  });

  it("cycles the step gauge style and turns the weather off and on", () => {
    const Settings = face.screens.settings;
    render(<Settings />);
    fireEvent.click(screen.getByText("Steps: Full"));
    fireEvent.click(screen.getByText("Steps: Half"));
    fireEvent.click(screen.getByText("Steps: Numeric"));
    expect(screen.getByText("Steps: Full")).toBeTruthy();
    fireEvent.click(screen.getByText("Weather: On"));
    fireEvent.click(screen.getByText("Weather: Off"));
    expect(screen.getByText("Weather: On")).toBeTruthy();
  });

  it("opens the settings on a long press and goes back from the tick", () => {
    vi.useFakeTimers();
    install_apps([face]);
    const stop_scheduler = start_scheduler();
    open(face);
    render(<OpenScreen />);
    act(() => {
      fireEvent.pointerDown(document.querySelector("[data-widget=screen]")!);
      vi.advanceTimersByTime(500);
    });
    expect(screen_stack.value.at(-1)).toBe("settings");
    act(() => screen.getAllByRole("button").at(-1)!.click());
    expect(screen_stack.value).toEqual(["main"]);
    vi.useRealTimers();
    stop_scheduler();
    open(null);
  });

  it("shows thousands of steps in the numeric style and seconds in the half style", () => {
    steps_today.value = 4321;
    const Settings = face.screens.settings;
    render(<Settings />);
    fireEvent.click(screen.getByText("Steps: Full"));
    cleanup();
    expect(labels_at(12, 0, 7)).toContain(":07");
    cleanup();
    render(<Settings />);
    fireEvent.click(screen.getByText("Steps: Half"));
    cleanup();
    expect(labels_at(12)).toContain("4K");
    expect(icon_symbols()).toContain(icon_symbol(icons.steps));
  });

  it("shows the temperature in the chosen units, and dashes before any forecast", () => {
    expect(labels_at(12)).toContain("--");
    cleanup();
    current_weather.value = { ...NO_WEATHER, temperature: 20, received_at: 1 };
    expect(labels_at(12)).toContain("20°");
    cleanup();
    settings.uses_metric_units.value = false;
    expect(labels_at(12)).toContain("68°");
    cleanup();
    current_weather.value = { ...NO_WEATHER, condition: "few_clouds", received_at: 1 };
    labels_at(23);
    expect(icon_symbols()).toContain(icon_symbol(icons.weather_night_cloudy));
  });

  it("moves the calendar up when the weather is hidden", () => {
    const weekday_top = () =>
      [...document.querySelectorAll<HTMLElement>("[data-widget=label]")].find((label) => label.textContent === "SUN")!.style.top;
    labels_at(12);
    expect(weekday_top()).toBe("99px");
    cleanup();
    const Settings = face.screens.settings;
    render(<Settings />);
    fireEvent.click(screen.getByText("Weather: On"));
    cleanup();
    expect(labels_at(12)).not.toContain("--");
    expect(weekday_top()).toBe("79px");
  });

  it("marks the afternoon on a 12 hour clock and shows midnight as 12", () => {
    settings.uses_24_hour_clock.value = false;
    let labels = labels_at(21, 30);
    expect(labels).toContain("09");
    expect(labels).toContain("P\nM");
    cleanup();
    labels = labels_at(0, 30);
    expect(labels).toContain("12");
    expect(labels).toContain("A\nM");
  });

  it("shows the charging and notification icons when they apply", () => {
    is_charging.value = true;
    unread_notifications.value = 1;
    labels_at(12);
    expect(icon_symbols()).toContain(icon_symbol(icons.charging));
    expect(icon_symbols()).toContain(icon_symbol(icons.notification));
  });
});
