// Plays the rally on a frozen clock and checks the time, the moving ball, the rackets and the serve after a miss.

import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { settings } from "battletime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, start_scheduler, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

let stop_scheduler = () => {};

/** The centre of the ball, from its yellow square. */
function ball(): { x: number; y: number } {
  const square = [...document.querySelectorAll<HTMLElement>("[data-widget=column]")].find(
    (element) => element.style.background === "rgb(255, 255, 0)",
  )!;
  return { x: parseInt(square.style.left) + 5, y: parseInt(square.style.top) + 5 };
}

/** The centres of the left and right rackets, from their orange bars. */
function racket_centres(): number[] {
  return [...document.querySelectorAll<HTMLElement>("[data-widget=column]")]
    .filter((element) => element.style.background === "rgb(255, 102, 0)")
    .map((element) => parseInt(element.style.top) + 16);
}

function play_step() {
  act(() => advance_clock(500));
}

function open_at(hour: number, minute: number) {
  freeze_clock(new Date(2026, 9, 11, hour, minute, 0).getTime());
  open(face);
  render(<OpenScreen />);
  play_step();
}

// ============================================================================
// Tests
// ============================================================================

describe("lite pong", () => {
  beforeEach(() => {
    install_apps([face]);
    stop_scheduler = start_scheduler();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    stop_scheduler();
    open(null);
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
  });

  it("shows the hour and minute over the date", () => {
    open_at(8, 5);
    expect(screen.getByText("08")).toBeTruthy();
    expect(screen.getByText("05")).toBeTruthy();
    expect(screen.getByText("Sun, 11 Oct")).toBeTruthy();
  });

  it("drops the leading zero on a 12 hour clock and shows midnight as 12", () => {
    settings.uses_24_hour_clock.value = false;
    open_at(20, 5);
    expect(screen.getByText("8")).toBeTruthy();
    cleanup();
    open(null);
    open_at(0, 5);
    expect(screen.getByText("12")).toBeTruthy();
  });

  it("serves the ball down and to the right, six pixels each step", () => {
    open_at(8, 5);
    expect(ball()).toEqual({ x: 120, y: 120 });
    play_step();
    expect(ball()).toEqual({ x: 126, y: 126 });
    play_step();
    expect(ball()).toEqual({ x: 132, y: 132 });
  });

  it("keeps the ball and rackets on the screen through a long rally", () => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    open_at(8, 5);
    for (let step = 0; step < 300; step++) {
      play_step();
      expect(ball().x).toBeGreaterThanOrEqual(0);
      expect(ball().x).toBeLessThanOrEqual(240);
      expect(ball().y).toBeGreaterThanOrEqual(0);
      expect(ball().y).toBeLessThanOrEqual(240);
      for (const centre of racket_centres()) {
        expect(centre).toBeGreaterThanOrEqual(15);
        expect(centre).toBeLessThanOrEqual(225);
      }
    }
  });

  it("waits after a missed ball, then serves it from the other racket", () => {
    const rolls = [0.1, 0.5, 0.75];
    let roll = 0;
    vi.spyOn(Math, "random").mockImplementation(() => rolls[roll++ % rolls.length]);
    open_at(8, 5);
    let previous = ball();
    let steps = 0;
    for (; steps < 500; steps++) {
      play_step();
      if (ball().x === previous.x && ball().y === previous.y) break;
      previous = ball();
    }
    expect(steps).toBeLessThan(500);
    const served_from = ball();
    expect([12, 228]).toContain(served_from.x);
    for (let step = 0; step < 68; step++) play_step();
    expect(ball()).toEqual(served_from);
    play_step();
    play_step();
    expect(ball()).not.toEqual(served_from);
  });
});
