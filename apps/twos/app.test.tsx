import { describe, expect, it } from "vitest";
import { board, can_move, move, slide, slide_line, slide_points, start_game } from "./app.tsx";

describe("2048 rules", () => {
  it("slides tiles to the edge and merges each pair once", () => {
    expect(slide_line([1, 1, 1, 1])).toEqual([2, 2, 0, 0, 8]);
    expect(slide_line([0, 1, 0, 1])).toEqual([2, 0, 0, 0, 4]);
    expect(slide_line([2, 1, 1, 0])).toEqual([2, 2, 0, 0, 4]);
    expect(slide_line([1, 2, 3, 4])).toEqual([1, 2, 3, 4, 0]);
  });

  it("moves the board in every direction", () => {
    const board = [1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
    expect(slide(board, "left").slice(0, 4)).toEqual([2, 0, 0, 0]);
    expect(slide(board, "right").slice(0, 4)).toEqual([0, 0, 0, 2]);
    expect(slide(board, "up")[0]).toBe(1);
    expect(slide(board, "up")[4]).toBe(2);
    expect(slide(board, "down")[12]).toBe(2);
    expect(slide(board, "down")[8]).toBe(1);
    expect(slide(board, "left")).toHaveLength(16);
    expect(slide_points(board, "left")).toBe(4);
    expect(slide_points(board, "up")).toBe(0);
  });

  it("sees when no move is left", () => {
    expect(can_move([1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(false);
    expect(can_move([1, 1, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(true);
  });

  it("starts with two small tiles", () => {
    start_game();
    const tiles = board.value.filter((level) => level !== 0);
    expect(tiles).toHaveLength(2);
    for (const level of tiles) expect([1, 2]).toContain(level);
  });

  it("adds one tile after a slide that changes the board, and none after one that does not", () => {
    board.value = [1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    move({ direction: "up" });
    expect(board.value).toEqual([1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    move({ direction: "left" });
    expect(board.value[0]).toBe(2);
    expect(board.value.filter((level) => level !== 0)).toHaveLength(2);
  });
});
