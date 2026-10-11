import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import { current_weather, now } from "neotime";
import weather from "./app.tsx";

afterEach(cleanup);

describe("weather", () => {
  beforeAll(() => {
    install_apps([weather]);
    start_scheduler();
  });

  it("shows the current weather and the days the phone sent", () => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    open(weather);
    render(<OpenScreen />);
    act(() => {
      // The system keeps the weather, the coming days included, so the app has nothing to store.
      current_weather.value = {
        temperature: 21,
        condition: "few_clouds",
        low: 12,
        high: 24,
        location: "Somewhere",
        received_at: now.value,
        sunrise: null,
        sunset: null,
        forecast: [
          { condition: "rain", low: 9, high: 17 },
          { condition: "snow", low: -3, high: 1 },
        ],
      };
    });
    expect(screen.getByText("Somewhere")).toBeTruthy();
    expect(screen.getByText("21°C")).toBeTruthy();
    expect(screen.getByText("Few clouds")).toBeTruthy();
    expect(screen.getByText("Fri")).toBeTruthy();
    expect(screen.getByText("-3")).toBeTruthy();
    expect(screen.queryByText("Sun")).toBeNull();
  });
});
