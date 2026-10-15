import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, SECOND } from "battletime";
import { advance_clock, effect_log, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

const START = Date.UTC(2026, 9, 11, 8, 0, 0);

function lap_or_reset_button(): HTMLButtonElement {
  return screen.getAllByRole("button")[0] as HTMLButtonElement;
}

function start_or_pause_button(): HTMLButtonElement {
  return screen.getAllByRole("button")[1] as HTMLButtonElement;
}

function time_color(): string {
  return (document.querySelector("[data-widget=label]") as HTMLElement).style.color;
}

describe("stopwatch", () => {
  beforeEach(() => {
    freeze_clock(START);
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

  it("starts cleared, with no lap or reset to take", () => {
    render(<OpenScreen />);
    expect(screen.getByText("00:00")).toBeTruthy();
    expect(lap_or_reset_button().disabled).toBe(true);
  });

  it("runs from the button and shows hundredths", () => {
    render(<OpenScreen />);
    act(() => start_or_pause_button().click());
    act(() => freeze_clock(START + 65 * SECOND + 470));
    expect(screen.getByText("01:05")).toBeTruthy();
    expect(screen.getByText("47")).toBeTruthy();
    expect(effect_log.value.some((effect) => effect.kind === "keep_screen_on" && effect.on === true)).toBe(true);
  });

  it("lists laps newest first with each lap's own time and the total", () => {
    render(<OpenScreen />);
    act(() => start_or_pause_button().click());
    act(() => advance_clock(10 * SECOND));
    act(() => lap_or_reset_button().click());
    act(() => advance_clock(15 * SECOND));
    act(() => lap_or_reset_button().click());
    expect(screen.getByText("#2")).toBeTruthy();
    expect(screen.getByText("#1")).toBeTruthy();
    expect(screen.getByText("00:15.00")).toBeTruthy();
    expect(screen.getByText("00:25.00")).toBeTruthy();
    expect(screen.getAllByText("00:10.00")).toHaveLength(2);
  });

  it("blinks the time while paused and resets from the stop button", () => {
    render(<OpenScreen />);
    act(() => start_or_pause_button().click());
    act(() => advance_clock(10 * SECOND));
    act(() => lap_or_reset_button().click());
    act(() => start_or_pause_button().click());
    const colors = [time_color()];
    act(() => advance_clock(SECOND));
    colors.push(time_color());
    expect(colors[0]).not.toBe(colors[1]);
    expect(screen.getByText("00:10")).toBeTruthy();
    act(() => lap_or_reset_button().click());
    expect(screen.getByText("00:00")).toBeTruthy();
    expect(screen.queryByText("#1")).toBeNull();
  });

  it("switches to hours after an hour and keeps every lap in the list", () => {
    render(<OpenScreen />);
    act(() => start_or_pause_button().click());
    for (let lap = 1; lap <= 4; lap++) {
      act(() => advance_clock(SECOND));
      act(() => lap_or_reset_button().click());
    }
    act(() => freeze_clock(START + 3605 * SECOND));
    expect(screen.getByText("1:00:05")).toBeTruthy();
    expect(screen.getByText("#1")).toBeTruthy();
    expect(screen.getByText("#4")).toBeTruthy();
  });

  it("keeps the latest twenty laps in a scrolling list", () => {
    render(<OpenScreen />);
    act(() => start_or_pause_button().click());
    for (let lap = 1; lap <= 21; lap++) {
      act(() => advance_clock(SECOND));
      act(() => lap_or_reset_button().click());
    }
    expect(document.querySelector("[data-widget=list]")).toBeTruthy();
    expect(screen.getByText("#21")).toBeTruthy();
    expect(screen.getByText("#2")).toBeTruthy();
    expect(screen.queryByText("#1")).toBeNull();
  });
});
