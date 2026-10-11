import { describe, expect, it } from "vitest";
import { is_solved, shuffled_board, tile_moved_by, SOLVED_BOARD } from "./app.tsx";

function inversions(board: number[]): number {
  const tiles = board.filter((tile) => tile !== 0);
  let count = 0;
  tiles.forEach((tile, place) => tiles.slice(place + 1).forEach((later) => (count += tile > later ? 1 : 0)));
  return count;
}

describe("15 puzzle rules", () => {
  it("only deals solvable boards", () => {
    for (let round = 0; round < 200; round++) {
      const board = shuffled_board();
      expect(board[15]).toBe(0);
      expect([...board].sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
      expect(inversions(board) % 2).toBe(0);
    }
  });

  it("moves the tile beside the gap, never across an edge", () => {
    expect(tile_moved_by(15, "left")).toBe(-1);
    expect(tile_moved_by(15, "right")).toBe(14);
    expect(tile_moved_by(15, "down")).toBe(11);
    expect(tile_moved_by(15, "up")).toBe(-1);
    expect(tile_moved_by(4, "right")).toBe(-1);
    expect(tile_moved_by(4, "left")).toBe(5);
  });

  it("knows a solved board", () => {
    expect(is_solved(SOLVED_BOARD)).toBe(true);
    expect(is_solved([2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0])).toBe(false);
  });
});
