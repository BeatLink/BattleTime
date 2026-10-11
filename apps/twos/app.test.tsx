import { describe, expect, it } from "vitest";
import { can_move, slide, slide_line } from "./app.tsx";

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
    expect(slide(board, "left")[16]).toBe(4);
  });

  it("sees when no move is left", () => {
    expect(can_move([1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(false);
    expect(can_move([1, 1, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(true);
  });
});
