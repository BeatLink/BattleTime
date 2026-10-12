// Checks the digits on both clock types, the weekday and day, and the weather icon once a forecast arrives.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { current_weather, settings, type CurrentWeather } from "neotime";
import { icon_symbol, icons } from "neotime/icons";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

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

function draw_at(hour: number, minute: number, second = 0): Element {
  freeze_clock(new Date(2026, 9, 11, hour, minute, second).getTime());
  const Face = face.screens.main;
  return render(<Face />).container;
}

function labels(container: Element): string[] {
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

describe("lite weather", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
    current_weather.value = NO_WEATHER;
  });

  it("shows the hour, minute and second over the weekday and day", () => {
    expect(labels(draw_at(20, 5, 9))).toEqual(["20", "05", "09", "SUN", "11"]);
  });

  it("counts hours from 1 to 12 on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    expect(labels(draw_at(20, 5))[0]).toBe("08");
    cleanup();
    expect(labels(draw_at(0, 5))[0]).toBe("12");
  });

  it("shows the weather icon only once the phone sends a forecast", () => {
    expect(draw_at(12, 0).querySelector("[data-widget=icon]")).toBeNull();
    cleanup();
    current_weather.value = { ...NO_WEATHER, condition: "rain", received_at: 1 };
    expect(draw_at(12, 0).querySelector("[data-widget=icon]")?.textContent).toBe(icon_symbol(icons.rain));
  });
});
