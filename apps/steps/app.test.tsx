import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { steps_today } from "neotime";
import { advance_clock, deliver, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import steps_app from "./app.tsx";

afterEach(cleanup);

describe("steps", () => {
  beforeAll(() => {
    install_apps([steps_app]);
    start_scheduler();
  });

  it("counts a trip, resets it and moves today into yesterday at midnight", () => {
    freeze_clock(new Date(2026, 0, 1, 23, 58, 0).getTime());
    open(steps_app);
    render(<OpenScreen />);
    act(() => { steps_today.value = 100; });
    act(() => { deliver("steps", { steps_today: 100 }); });
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
});
