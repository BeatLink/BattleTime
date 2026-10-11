import { describe, expect, it } from "vitest";
import { choose_column, wins_at, EMPTY_BOARD } from "./app.tsx";

function board_with(pieces: [number, number, number][]): number[] {
  const board = [...EMPTY_BOARD];
  for (const [column, row, piece] of pieces) board[row * 7 + column] = piece;
  return board;
}

function heights_of(board: number[]): number[] {
  return [0, 1, 2, 3, 4, 5, 6].map((column) => [0, 1, 2, 3, 4, 5].filter((row) => board[row * 7 + column] !== 0).length);
}

describe("four in a row", () => {
  it("sees four in a row across, down and on both diagonals", () => {
    const across = board_with([[0, 0, 1], [1, 0, 1], [2, 0, 1]]);
    expect(wins_at(across, 3, 0, 1)).toBe(true);
    expect(wins_at(across, 3, 0, 2)).toBe(false);
    const down = board_with([[4, 0, 2], [4, 1, 2], [4, 2, 2]]);
    expect(wins_at(down, 4, 3, 2)).toBe(true);
    const diagonal = board_with([[0, 0, 1], [1, 1, 1], [3, 3, 1]]);
    expect(wins_at(diagonal, 2, 2, 1)).toBe(true);
    const other_diagonal = board_with([[6, 0, 1], [5, 1, 1], [4, 2, 1]]);
    expect(wins_at(other_diagonal, 3, 3, 1)).toBe(true);
  });

  it("takes a win and blocks a threat at every level", () => {
    const can_win = board_with([[0, 0, 2], [0, 1, 2], [0, 2, 2], [1, 0, 1], [2, 0, 1]]);
    const must_block = board_with([[6, 0, 1], [6, 1, 1], [6, 2, 1], [0, 0, 2], [3, 0, 2]]);
    for (const depth of [1, 2, 3, 4]) {
      expect(choose_column(can_win, heights_of(can_win), 5, 2, depth)).toBe(0);
      expect(choose_column(must_block, heights_of(must_block), 5, 2, depth)).toBe(6);
    }
  });

  it("sets up a double threat when it looks far enough ahead", () => {
    const board = board_with([[2, 0, 2], [3, 0, 2], [0, 0, 1], [0, 1, 1]]);
    expect([1, 4]).toContain(choose_column(board, heights_of(board), 4, 2, 3));
  });
});
