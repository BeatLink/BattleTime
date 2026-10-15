import { act, cleanup, fireEvent, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { advance_clock, effect_log, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app, { next_generation, ACORN } from "./app.tsx";

function board_with(cells: number[]): number[] {
  return Array.from({ length: 256 }, (_, cell) => (cells.includes(cell) ? 1 : 0));
}

function live_cells(board: number[]): number[] {
  return board.flatMap((alive, cell) => (alive ? [cell] : []));
}

describe("game of life", () => {
  it("turns a blinker", () => {
    expect(live_cells(next_generation(board_with([17, 18, 19]), 1))).toEqual([2, 18, 34]);
  });

  it("keeps a block still", () => {
    expect(live_cells(next_generation(board_with([0, 1, 16, 17]), 1))).toEqual([0, 1, 16, 17]);
  });

  it("wraps around the edges", () => {
    expect(live_cells(next_generation(board_with([15, 0, 1]), 1))).toEqual([0, 16, 240]);
  });

  it("starts from a seven cell acorn", () => {
    expect(live_cells(ACORN)).toHaveLength(7);
  });

  it("colours every live cell with the generation's colour, whatever colour its neighbours had", () => {
    const board = board_with([17, 18, 19]).map((alive, cell) => (alive ? 1 + (cell % 3) : 0));
    const next = next_generation(board, 5);
    expect(live_cells(next)).toEqual([2, 18, 34]);
    expect([next[2], next[18], next[34]]).toEqual([5, 5, 5]);
  });
});

let stop_scheduler = () => {};

/** The cells drawn in any colour but the dead cells' black. */
function drawn_live_cells(): number[] {
  return [...document.querySelectorAll("[data-widget=cell-grid] rect")].flatMap((cell, place) =>
    cell.getAttribute("fill") !== "#000000" ? [place] : [],
  );
}

function screen_element(): HTMLElement {
  return document.querySelector("[data-widget=screen]") as HTMLElement;
}

describe("game of life screen", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
    stop_scheduler();
    open(null);
  });

  it("keeps the screen on and draws a new generation in a new colour every tick", () => {
    expect(effect_log.value.some((effect) => effect.kind === "keep_screen_on" && effect.on === true)).toBe(true);
    expect(drawn_live_cells()).toEqual(live_cells(ACORN));
    const first_colour = document.querySelectorAll("[data-widget=cell-grid] rect")[118].getAttribute("fill");
    act(() => advance_clock(2000));
    const after = drawn_live_cells();
    expect(after).toEqual(live_cells(next_generation(ACORN, 1)));
    expect(document.querySelectorAll("[data-widget=cell-grid] rect")[after[0]].getAttribute("fill")).not.toBe(first_colour);
  });

  it("plants the acorn again on a tap", () => {
    act(() => advance_clock(3000));
    expect(drawn_live_cells()).not.toEqual(live_cells(ACORN));
    act(() => screen_element().click());
    expect(drawn_live_cells()).toEqual(live_cells(ACORN));
  });

  it("scatters a random soup on a long press", () => {
    vi.useFakeTimers();
    vi.spyOn(Math, "random").mockReturnValue(0);
    act(() => {
      fireEvent.pointerDown(screen_element());
      vi.advanceTimersByTime(500);
    });
    expect(drawn_live_cells()).toHaveLength(256);
  });
});
