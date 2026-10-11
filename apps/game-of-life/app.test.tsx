import { describe, expect, it } from "vitest";
import { next_generation, ACORN } from "./app.tsx";

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
