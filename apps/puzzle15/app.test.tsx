import { describe, expect, it } from "vitest";
import { board as dealt_board, is_solved, slide, start_game, tile_moved_by, SOLVED_BOARD } from "./app.tsx";

function inversions(board: number[]): number {
  const tiles = board.filter((tile) => tile !== 0);
  let count = 0;
  tiles.forEach((tile, place) => tiles.slice(place + 1).forEach((later) => (count += tile > later ? 1 : 0)));
  return count;
}

describe("15 puzzle rules", () => {
  it("only deals solvable boards", () => {
    for (let round = 0; round < 200; round++) {
      start_game();
      const board = dealt_board.value;
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

describe("15 puzzle moves", () => {
  it("slides the tile beside the gap into it, and stops once solved", () => {
    dealt_board.value = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0, 15];
    slide({ direction: "left" });
    expect(dealt_board.value).toEqual(SOLVED_BOARD);
    slide({ direction: "right" });
    expect(dealt_board.value).toEqual(SOLVED_BOARD);
  });

  it("swaps only the two cells", () => {
    dealt_board.value = [1, 2, 3, 4, 5, 0, 7, 8, 9, 6, 11, 12, 13, 10, 14, 15];
    slide({ direction: "up" });
    expect(dealt_board.value).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 11, 12, 13, 10, 14, 15]);
  });
});
