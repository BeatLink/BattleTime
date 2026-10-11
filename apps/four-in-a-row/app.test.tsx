import { describe, expect, it } from "vitest";
import {
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
