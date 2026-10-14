import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reset_persisted } from "neotime";
import {
  advance_clock,
  effect_log,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  screen_stack,
  start_scheduler,
  swipe,
} from "neotime/preview";
import app, {
  begin_search,
  board,
  chosen_column,
  filled,
  heights,
  search_depth,
  search_step,
  wins_at,
  EMPTY_BOARD,
  ROW_WIDTH,
} from "./app.tsx";

function board_with(pieces: [number, number, number][]): number[] {
  const cells = [...EMPTY_BOARD];
  for (const [column, row, piece] of pieces) cells[row * ROW_WIDTH + column] = piece;
  return cells;
}

function heights_of(cells: number[]): number[] {
  return [0, 1, 2, 3, 4, 5, 6].map(
    (column) => [0, 1, 2, 3, 4, 5].filter((row) => cells[row * ROW_WIDTH + column] !== 0).length,
  );
}

/** Runs the watch's search on a board to the end, as the tick would, and returns its column. */
function choose_column(cells: number[], filled_count: number, reach: number): number {
  board.value = cells;
  heights.value = heights_of(cells);
  filled.value = filled_count;
  begin_search(reach);
  let steps = 0;
  while (search_depth.value >= 0) {
    search_step();
    steps = steps + 1;
    expect(steps).toBeLessThan(500);
  }
  return chosen_column.value;
}

describe("four in a row", () => {
  it("sees four in a row across, down and on both diagonals", () => {
    const across = board_with([[0, 0, 1], [1, 0, 1], [2, 0, 1]]);
    expect(wins_at(across, 3, 1)).toBe(true);
    expect(wins_at(across, 3, 2)).toBe(false);
    const down = board_with([[4, 0, 2], [4, 1, 2], [4, 2, 2]]);
    expect(wins_at(down, 3 * ROW_WIDTH + 4, 2)).toBe(true);
    const diagonal = board_with([[0, 0, 1], [1, 1, 1], [3, 3, 1]]);
    expect(wins_at(diagonal, 2 * ROW_WIDTH + 2, 1)).toBe(true);
    const other_diagonal = board_with([[6, 0, 1], [5, 1, 1], [4, 2, 1]]);
    expect(wins_at(other_diagonal, 3 * ROW_WIDTH + 3, 1)).toBe(true);
  });

  it("never joins a line across the edge of the board", () => {
    const wrapping = board_with([[5, 0, 1], [6, 0, 1], [0, 1, 1]]);
    expect(wins_at(wrapping, 1 * ROW_WIDTH + 1, 1)).toBe(false);
  });

  it("takes a win and blocks a threat at every level", () => {
    const can_win = board_with([[0, 0, 2], [0, 1, 2], [0, 2, 2], [1, 0, 1], [2, 0, 1]]);
    const must_block = board_with([[6, 0, 1], [6, 1, 1], [6, 2, 1], [0, 0, 2], [3, 0, 2]]);
    for (const reach of [1, 2, 3, 4]) {
      expect(choose_column(can_win, 5, reach)).toBe(0);
      expect(choose_column(must_block, 5, reach)).toBe(6);
    }
  });

  it("takes its own win before blocking", () => {
    const both = board_with([[0, 0, 2], [0, 1, 2], [0, 2, 2], [6, 0, 1], [6, 1, 1], [6, 2, 1]]);
    expect(choose_column(both, 6, 2)).toBe(0);
  });

  it("sets up a double threat when it looks far enough ahead", () => {
    const cells = board_with([[2, 0, 2], [3, 0, 2], [0, 0, 1], [0, 1, 1]]);
    expect([1, 4]).toContain(choose_column(cells, 4, 3));
  });
});

let stop_scheduler = () => {};

/** Sets the position the next move is played on. */
function set_position(cells: number[]) {
  board.value = cells;
  heights.value = heights_of(cells);
  filled.value = cells.filter((piece) => piece !== 0).length;
}

function last_vibration(): unknown {
  return effect_log.value.filter((effect) => effect.kind === "vibrate").at(-1)?.milliseconds;
}

function aim_marker(): HTMLElement | null {
  return document.querySelector("[data-widget=screen] > [data-widget=column][style*='border-radius: 15px']");
}

describe("four in a row screens", () => {
  beforeEach(() => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    stop_scheduler();
    open(null);
  });

  it("chooses a level on the menu and starts a game on a swipe down", () => {
    expect(screen.getByText("Won 0%")).toBeTruthy();
    act(() => screen.getByText("2").click());
    expect(screen.getByText("at level 2")).toBeTruthy();
    act(() => swipe("down"));
    expect(screen_stack.value.at(-1)).toBe("game");
    expect(filled.value).toBe(0);
  });

  it("starts a game from a tap on the title card", () => {
    act(() => screen.getByText("Four in a Row").click());
    expect(screen_stack.value.at(-1)).toBe("game");
  });

  it("leaves the menu on a right swipe", () => {
    act(() => swipe("right"));
    expect(screen_stack.value).toEqual(["main"]);
  });

  it("aims at the tapped column and drops there on a swipe down, then the watch answers", () => {
    act(() => swipe("down"));
    expect(aim_marker()?.style.left).toBe("105px");
    act(() => (document.querySelector("[data-widget=screen]") as HTMLElement).click());
    expect(aim_marker()?.style.left).toBe("3px");
    act(() => swipe("down"));
    expect(board.value[0]).toBe(1);
    expect(board.value[ROW_WIDTH]).toBe(2);
    expect(filled.value).toBe(2);
  });

  it("buzzes instead of dropping into a full column", () => {
    act(() => swipe("down"));
    set_position(board_with([[3, 0, 1], [3, 1, 2], [3, 2, 1], [3, 3, 2], [3, 4, 1], [3, 5, 2]]));
    act(() => swipe("down"));
    expect(filled.value).toBe(6);
    expect(last_vibration()).toBe(50);
  });

  it("declares a win, counts it and goes back to the menu", () => {
    act(() => swipe("down"));
    set_position(board_with([[3, 0, 1], [3, 1, 1], [3, 2, 1], [0, 0, 2], [1, 0, 2], [6, 0, 2]]));
    act(() => swipe("down"));
    expect(screen.getByText("You win!")).toBeTruthy();
    expect(last_vibration()).toBe(100);
    act(() => swipe("down"));
    expect(screen_stack.value).toEqual(["main"]);
    expect(screen.getByText("Won 100%")).toBeTruthy();
  });

  it("lets the watch open the next game and declares its win", () => {
    act(() => swipe("down"));
    set_position(board_with([[3, 0, 1], [3, 1, 1], [3, 2, 1], [0, 0, 2], [1, 0, 2], [6, 0, 2]]));
    act(() => swipe("down"));
    act(() => swipe("left"));
    act(() => screen.getByText("Four in a Row").click());
    expect(board.value[0]).toBe(2);
    expect(filled.value).toBe(1);
    set_position(board_with([[6, 0, 1], [6, 1, 1], [5, 0, 2], [5, 1, 2], [5, 2, 2], [0, 0, 1]]));
    act(() => swipe("down"));
    expect(screen.getByText("You lose!")).toBeTruthy();
    expect(last_vibration()).toBe(300);
  });

  it("shows that it is thinking while a deep search runs over several ticks", () => {
    act(() => screen.getByText("3").click());
    act(() => swipe("down"));
    set_position(board_with([[0, 0, 1], [6, 0, 2], [1, 0, 1], [5, 0, 2]]));
    act(() => swipe("down"));
    expect(screen.getByText("Thinking...")).toBeTruthy();
    expect(aim_marker()).toBeNull();
    act(() => swipe("down"));
    expect(last_vibration()).toBe(50);
    for (let tick = 0; tick < 200 && search_depth.value >= 0; tick++) act(() => advance_clock(100));
    expect(screen.queryByText("Thinking...")).toBeNull();
    expect(filled.value).toBe(6);
  });

  it("calls a full board without a line a draw", () => {
    act(() => swipe("down"));
    const pattern = [[1, 1, 2, 2, 1, 1, 2], [2, 2, 1, 1, 2, 2, 1]];
    const pieces: [number, number, number][] = [];
    for (let row = 0; row < 6; row++) {
      for (let column = 0; column < 7; column++) {
        if (row !== 5 || column !== 3) pieces.push([column, row, pattern[row % 2][column]]);
      }
    }
    set_position(board_with(pieces));
    act(() => swipe("down"));
    expect(screen.getByText("Draw")).toBeTruthy();
  });
});
