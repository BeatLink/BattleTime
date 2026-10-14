import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { empty_step_history, step_history, steps_today } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, start_scheduler, swipe } from "neotime/preview";
import steps_app from "./app.tsx";

afterEach(cleanup);

describe("steps", () => {
  beforeAll(() => {
    install_apps([steps_app]);
    start_scheduler();
  });

  beforeEach(() => {
    freeze_clock(new Date(2026, 0, 1, 23, 58, 0).getTime());
    act(() => {
      steps_today.value = 0;
      step_history.value = empty_step_history();
    });
  });

  it("counts a trip, resets it and moves today into yesterday at midnight", () => {
    open(steps_app);
    render(<OpenScreen />);
    act(() => { steps_today.value = 150; });
    expect(screen.getByText("Trip: 150")).toBeTruthy();
    act(() => { screen.getByText("Reset").click(); });
    expect(screen.getByText("Trip: 0")).toBeTruthy();
    act(() => { steps_today.value = 170; });
    act(() => { advance_clock(3 * 60 * 1000); });
    expect(screen.getByText("Yest: 170")).toBeTruthy();
    act(() => { steps_today.value = 5; });
    expect(screen.getByText("Trip: 25")).toBeTruthy();
  });

  it("shows a long trip as its limit", () => {
    act(() => { step_history.value = { ...step_history.value, trip: 123_456 }; });
    open(steps_app);
    render(<OpenScreen />);
    expect(screen.getByText("Trip: 99999+")).toBeTruthy();
  });

  it("pages left to the week and a day, opens a day from the week, and steps between days with buttons", () => {
    open(steps_app);
    render(<OpenScreen />);
    act(() => { steps_today.value = 900; });
    act(() => { advance_clock(3 * 60 * 1000); });
    act(() => { swipe("left"); });
    expect(screen.getByText("Step history")).toBeTruthy();
    act(() => { swipe("left"); });
    expect(screen.getByText("Today")).toBeTruthy();
    act(() => { swipe("right"); });
    act(() => { screen.getByText("Thu").click(); });
    expect(screen.getByText("Yesterday")).toBeTruthy();
    expect(screen.getByText("900")).toBeTruthy();
    act(() => { screen.getByText("<").click(); });
    expect(screen.getByText("Wednesday")).toBeTruthy();
    act(() => { screen.getByText(">").click(); });
    act(() => { screen.getByText(">").click(); });
    expect(screen.getByText("Today")).toBeTruthy();
    act(() => { screen.getByText(">").click(); });
    expect(screen.getByText("Today")).toBeTruthy();
    act(() => { swipe("right"); });
    expect(screen.getByText("Step history")).toBeTruthy();
  });
});
