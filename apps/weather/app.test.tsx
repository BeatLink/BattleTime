import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { deliver, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
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
      deliver("weather", {
        temperature: 21,
        condition: "few_clouds",
        low: 12,
        high: 24,
        location: "Somewhere",
        days: [
          { condition: "rain", low: 9, high: 17 },
          { condition: "snow", low: -3, high: 1 },
        ],
      });
    });
    expect(screen.getByText("21°C")).toBeTruthy();
    expect(screen.getByText("Few clouds")).toBeTruthy();
    expect(screen.getByText("Fri")).toBeTruthy();
    expect(screen.getByText("-3")).toBeTruthy();
    expect(screen.queryByText("Sun")).toBeNull();
  });
});
