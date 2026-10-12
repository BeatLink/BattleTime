import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { heart_rate_status } from "neotime";
import { deliver, effect_log, install_apps, open, OpenScreen, press_button, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

describe("heart", () => {
  beforeEach(() => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    stop_scheduler();
    open(null);
  });

  it("starts measuring on open and shows the reading with a chart", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    expect(effect_log.value.some((effect) => effect.kind === "start_heart_rate")).toBe(true);
    expect(screen.getByText("060")).toBeTruthy();
    expect(screen.getByText("Measuring...")).toBeTruthy();
    expect(screen.getByText("Stop")).toBeTruthy();
    expect(container.querySelector("[data-widget=chart]")).not.toBeNull();
  });

  it("stops on the button and starts again with the side button", () => {
    open(app);
    render(<OpenScreen />);
    act(() => screen.getByText("Stop").click());
    expect(screen.getByText("Stopped")).toBeTruthy();
    expect(screen.getByText("---")).toBeTruthy();
    expect(effect_log.value.some((effect) => effect.kind === "stop_heart_rate")).toBe(true);
    act(() => deliver("heart_rate_reading", { beats_per_minute: 99, confidence: 90 }));
    expect(screen.getByText("---")).toBeTruthy();
    act(() => press_button());
    expect(screen.getByText("Stop")).toBeTruthy();
    expect(screen.getByText("060")).toBeTruthy();
  });

  it("explains what the sensor is doing before it has a reading", () => {
    open(app);
    render(<OpenScreen />);
    act(() => { heart_rate_status.value = 1; });
    expect(screen.getByText("No touch detected")).toBeTruthy();
    expect(screen.getByText("---")).toBeTruthy();
    act(() => { heart_rate_status.value = 2; });
    expect(screen.getByText("Not enough data, please wait...")).toBeTruthy();
    act(() => { heart_rate_status.value = 0; });
    expect(screen.getByText("Starting...")).toBeTruthy();
  });

  it("charts only readings taken while the sensor is measuring", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    const chart_points = () => container.querySelector("[data-widget=chart] polyline")?.getAttribute("points") ?? "";
    const before = chart_points();
    act(() => { heart_rate_status.value = 2; });
    act(() => deliver("heart_rate_reading", { beats_per_minute: 120, confidence: 10 }));
    expect(chart_points()).toBe(before);
    act(() => { heart_rate_status.value = 3; });
    act(() => deliver("heart_rate_reading", { beats_per_minute: 120, confidence: 90 }));
    expect(screen.getByText("120")).toBeTruthy();
    expect(chart_points()).not.toBe(before);
  });

  it("stops the sensor when it closes", () => {
    open(app);
    open(null);
    expect(effect_log.value.at(-1)?.kind).toBe("keep_screen_on");
    expect(effect_log.value.some((effect) => effect.kind === "stop_heart_rate")).toBe(true);
  });
});
