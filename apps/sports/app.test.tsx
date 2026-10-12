import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, steps_today, SECOND } from "neotime";
import { advance_clock, effect_log, freeze_clock, install_apps, open, OpenScreen, press_button, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function tap_screen() {
  act(() => (document.querySelector("[data-widget=screen]") as HTMLElement).click());
}

describe("sports", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    steps_today.value = 1000;
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("times a run and counts the steps taken during it", () => {
    render(<OpenScreen />);
    act(() => press_button());
    act(() => advance_clock(12 * SECOND));
    act(() => { steps_today.value = 1040; });
    expect(screen.getByText("00:12.00")).toBeTruthy();
    expect(screen.getByText("40")).toBeTruthy();
    expect(effect_log.value.some((effect) => effect.kind === "keep_screen_on" && effect.on === true)).toBe(true);
  });

  it("holds the time and steps while stopped and adds the next run to them", () => {
    render(<OpenScreen />);
    act(() => press_button());
    act(() => advance_clock(10 * SECOND));
    act(() => { steps_today.value = 1010; });
    act(() => screen.getByRole("button").click());
    act(() => advance_clock(30 * SECOND));
    act(() => { steps_today.value = 1500; });
    expect(screen.getByText("00:10.00")).toBeTruthy();
    expect(screen.getByText("10")).toBeTruthy();
    act(() => press_button());
    act(() => advance_clock(5 * SECOND));
    act(() => { steps_today.value = 1505; });
    expect(screen.getByText("00:15.00")).toBeTruthy();
    expect(screen.getByText("15")).toBeTruthy();
  });

  it("clears on a tap only once stopped", () => {
    render(<OpenScreen />);
    act(() => press_button());
    act(() => advance_clock(10 * SECOND));
    tap_screen();
    expect(screen.getByText("00:10.00")).toBeTruthy();
    act(() => press_button());
    tap_screen();
    expect(screen.getByText("00:00.00")).toBeTruthy();
    expect(screen.getByText("0")).toBeTruthy();
  });

  it("keeps counting steps when the day's count starts again at midnight", () => {
    render(<OpenScreen />);
    act(() => press_button());
    act(() => { steps_today.value = 25; });
    expect(screen.getByText("25")).toBeTruthy();
  });

  it("shows hours once a run passes one", () => {
    render(<OpenScreen />);
    act(() => press_button());
    act(() => freeze_clock(Date.UTC(2026, 9, 11, 9, 0, 5)));
    expect(screen.getByText("1:00:05")).toBeTruthy();
  });
});
