// Checks where the step gauge's needle points, and which weather icon the sidebar shows by day and by night.

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { current_weather, reset_persisted, settings, steps_today } from "neotime";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face, { step_gauge_value } from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

const NO_WEATHER = {
  temperature: 0,
  condition: "clear",
  low: 0,
  high: 0,
  location: "",
  received_at: null,
  sunrise: null,
  sunset: null,
} as const;

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
