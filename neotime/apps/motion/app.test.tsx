import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { steps_today } from "neotime";
import { deliver, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

describe("motion", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("shows the latest reading of each axis and the steps", () => {
    act(() => { steps_today.value = 1234; });
    act(() => deliver("acceleration", { x: 12, y: -340, z: -980 }));
    expect(screen.getByText("12")).toBeTruthy();
    expect(screen.getByText("-340")).toBeTruthy();
    expect(screen.getByText("-980")).toBeTruthy();
    expect(screen.getByText("Steps 1234")).toBeTruthy();
  });

  it("charts the last ten readings of each axis, oldest first", () => {
    for (let reading = 1; reading <= 12; reading++) {
      act(() => deliver("acceleration", { x: reading * 100, y: 0, z: 0 }));
    }
    const x_points = document.querySelector("[data-widget=chart] polyline")!.getAttribute("points")!.split(" ");
    expect(x_points).toHaveLength(10);
    const heights = x_points.map((point) => Number(point.split(",")[1]));
    expect(heights[0]).toBeGreaterThan(heights[9]);
    expect(screen.getByText("1200")).toBeTruthy();
  });
});
