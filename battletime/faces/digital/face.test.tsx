// Checks the two-tone digits on both clock types, the status icons, the weather line and the heart rate and steps.

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import {
  current_weather,
  heart_rate,
  is_heart_rate_running,
  is_phone_connected,
  next_alarm,
  settings,
  steps_today,
  unread_notifications,
  type CurrentWeather,
} from "battletime";
import { icon_symbol, icons } from "battletime/icons";
import { freeze_clock, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

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

function instant_at(hour: number, minute = 0): number {
  return new Date(2026, 9, 11, hour, minute, 0).getTime();
}

/** The face's labels and icon symbols, drawn at an hour and minute. */
function draw_at(hour: number, minute = 0): { labels: string[]; icons: string[] } {
  freeze_clock(instant_at(hour, minute));
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return {
    labels: [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? ""),
    icons: [...container.querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent ?? ""),
  };
}

/** The five big characters of the time, tens of hours first. */
function digits(labels: string[]): string {
  const colon = labels.indexOf(":");
  return labels.slice(colon - 2, colon + 3).join("");
}

// ============================================================================
// Tests
// ============================================================================

describe("digital", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
    settings.uses_metric_units.value = true;
    current_weather.value = NO_WEATHER;
    unread_notifications.value = 0;
    next_alarm.value = null;
    is_phone_connected.value = true;
    is_heart_rate_running.value = false;
    steps_today.value = 0;
  });

  it("shows the time with a leading zero and the date day first on a 24 hour clock", () => {
    const { labels } = draw_at(8, 5);
    expect(digits(labels)).toBe("08:05");
    expect(labels).toContain("Sun 11 Oct 2026");
    expect(labels).not.toContain("AM");
  });

  it("drops the leading zero, adds AM or PM and puts the month first on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    let drawn = draw_at(21, 30);
    expect(digits(drawn.labels)).toBe(" 9:30");
    expect(drawn.labels).toContain("PM");
    expect(drawn.labels).toContain("Sun Oct 11 2026");
    cleanup();
    drawn = draw_at(0, 15);
    expect(digits(drawn.labels)).toBe("12:15");
    expect(drawn.labels).toContain("AM");
  });

  it("shows the notification, connection and alarm icons only when they apply", () => {
    let drawn = draw_at(12);
    expect(drawn.icons).toContain(icon_symbol(icons.bluetooth));
    expect(drawn.icons).not.toContain(icon_symbol(icons.notification));
    expect(drawn.icons).not.toContain(icon_symbol(icons.bell));
    cleanup();
    unread_notifications.value = 2;
    next_alarm.value = instant_at(13);
    is_phone_connected.value = false;
    drawn = draw_at(12);
    expect(drawn.icons).toContain(icon_symbol(icons.notification));
    expect(drawn.icons).toContain(icon_symbol(icons.bell));
    expect(drawn.icons).not.toContain(icon_symbol(icons.bluetooth));
  });

  it("shows the weather once the phone sends it, with the moon at night", () => {
    expect(draw_at(12).labels).not.toContain("21°C");
    cleanup();
    current_weather.value = { ...NO_WEATHER, temperature: 21, received_at: 1 };
    let drawn = draw_at(12);
    expect(drawn.labels).toContain("21°C");
    expect(drawn.icons).toContain(icon_symbol(icons.clear));
    cleanup();
    expect(draw_at(22).icons).toContain(icon_symbol(icons.weather_night_clear));
    cleanup();
    current_weather.value = { ...NO_WEATHER, condition: "few_clouds", received_at: 1 };
    expect(draw_at(3).icons).toContain(icon_symbol(icons.weather_night_cloudy));
    cleanup();
    current_weather.value = { ...NO_WEATHER, condition: "rain", received_at: 1 };
    expect(draw_at(22).icons).toContain(icon_symbol(icons.rain));
  });

  it("takes night from the phone's sunset and sunrise when it sends them", () => {
    current_weather.value = {
      ...NO_WEATHER,
      received_at: 1,
      sunrise: instant_at(7, 30),
      sunset: instant_at(20, 15),
    };
    expect(draw_at(19).icons).toContain(icon_symbol(icons.clear));
    cleanup();
    expect(draw_at(20, 15).icons).toContain(icon_symbol(icons.weather_night_clear));
    cleanup();
    expect(draw_at(7, 29).icons).toContain(icon_symbol(icons.weather_night_clear));
  });

  it("shows the heart rate only while the sensor runs, and the steps", () => {
    heart_rate.value = 72;
    steps_today.value = 4321;
    expect(draw_at(12).labels).not.toContain("72");
    cleanup();
    is_heart_rate_running.value = true;
    draw_at(12);
    expect(screen.getByText("72")).toBeTruthy();
    expect(screen.getByText("4321")).toBeTruthy();
  });
});
