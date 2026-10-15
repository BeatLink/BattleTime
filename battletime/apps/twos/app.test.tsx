import { describe, expect, it } from "vitest";
import { board, can_slide, move, slid_board, slide_changes, slide_points, start_game } from "./app.tsx";

describe("2048", () => {
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

  it("merges a pair into the next tile up", () => {
    board.value = [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    move({ direction: "left" });
    expect(board.value[0]).toBe(2);
  });

  it("slides tiles to the edge and joins each pair once", () => {
    const row = (cells: number[]) => [...cells, ...Array(12).fill(0)];
    expect(slid_board(row([1, 1, 1, 1]), "left").slice(0, 4)).toEqual([2, 2, 0, 0]);
    expect(slid_board(row([0, 1, 0, 1]), "left").slice(0, 4)).toEqual([2, 0, 0, 0]);
    expect(slid_board(row([2, 1, 1, 0]), "left").slice(0, 4)).toEqual([2, 2, 0, 0]);
    expect(slid_board(row([1, 2, 3, 4]), "left").slice(0, 4)).toEqual([1, 2, 3, 4]);
    expect(slide_points(row([1, 1, 1, 1]), "left")).toBe(8);
  });

  it("moves the board in every direction", () => {
    const tiles = [1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
    expect(slid_board(tiles, "left").slice(0, 4)).toEqual([2, 0, 0, 0]);
    expect(slid_board(tiles, "right").slice(0, 4)).toEqual([0, 0, 0, 2]);
    expect(slid_board(tiles, "up")[4]).toBe(2);
    expect(slid_board(tiles, "down")[12]).toBe(2);
    expect(slid_board(tiles, "down")[8]).toBe(1);
    expect(slide_points(tiles, "left")).toBe(4);
    expect(slide_points(tiles, "up")).toBe(0);
    expect(slide_changes(tiles, "left")).toBe(true);
    expect(slide_changes([1, 2, 3, 4, ...Array(12).fill(0)], "up")).toBe(false);
  });

  it("sees when no slide is left", () => {
    expect(can_slide([1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(false);
    expect(can_slide([1, 1, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1])).toBe(true);
    expect(can_slide([1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 2])).toBe(true);
    expect(can_slide(Array(16).fill(0))).toBe(false);
  });
});
