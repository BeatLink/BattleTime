import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reset_persisted, SECOND } from "battletime";
import { advance_clock, deliver, effect_log, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function labels(container: Element): string[] {
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

/** Turns the count or sides roller one step by tapping the row beside the chosen one. */
function step_roller(container: Element, place: number, offset = 1) {
  const roller = container.querySelectorAll("[data-widget=roller]")[place];
  act(() => (roller.querySelector(`[data-roller-offset='${offset}']`) as HTMLElement).click());
}

function tap_roll(container: Element) {
  act(() => (container.querySelector("[data-widget=button]") as HTMLElement).click());
}

function rolls_so_far(): number {
  return effect_log.value.filter((effect) => effect.kind === "vibrate").length;
}

describe("dice", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
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

  it("adds up every die and lists each face", () => {
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    open(app);
    const { container } = render(<OpenScreen />);
    step_roller(container, 0);
    step_roller(container, 0);
    tap_roll(container);
    expect(labels(container)).toContain("12");
    expect(labels(container)).toContain("4+4+4");
    expect(rolls_so_far()).toBe(1);
  });

  it("calls heads or tails for a single two-sided die", () => {
    const random = vi.spyOn(Math, "random").mockReturnValue(0);
    open(app);
    const { container } = render(<OpenScreen />);
    for (let step = 0; step < 4; step++) step_roller(container, 1, -1);
    expect(container.querySelector("[data-roller-offset='0']")?.textContent).toBe("1");
    expect(container.querySelectorAll("[data-roller-offset='0']")[1].textContent).toBe("02");
    tap_roll(container);
    expect(labels(container)).toContain("HEADS");
    random.mockReturnValue(0.9);
    tap_roll(container);
    expect(labels(container)).toContain("TAILS");
  });

  it("changes the result's colour with every roll", () => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    open(app);
    const { container } = render(<OpenScreen />);
    const total_color = () => (container.querySelector("[data-widget=label][style*='42px']") as HTMLElement).style.color;
    const colors = [total_color()];
    for (let roll = 0; roll < 3; roll++) {
      tap_roll(container);
      colors.push(total_color());
    }
    expect(colors[1]).not.toBe(colors[0]);
    expect(colors[2]).not.toBe(colors[1]);
    expect(colors[3]).toBe(colors[0]);
  });

  it("rolls on a shake, but never within a second of the last roll", () => {
    open(app);
    render(<OpenScreen />);
    act(() => deliver("motion", { kind: "shake" }));
    expect(rolls_so_far()).toBe(0);
    act(() => advance_clock(SECOND));
    act(() => deliver("motion", { kind: "shake" }));
    expect(rolls_so_far()).toBe(1);
    act(() => deliver("motion", { kind: "shake" }));
    expect(rolls_so_far()).toBe(1);
    act(() => advance_clock(SECOND));
    act(() => deliver("motion", { kind: "shake" }));
    expect(rolls_so_far()).toBe(2);
  });
});
