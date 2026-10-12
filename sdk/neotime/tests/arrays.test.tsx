import { describe, expect, it } from "vitest";
import {
  arrays_equal,
  can_slide,
  count_of,
  life_step,
  line_through,
  nth_index_of,
  slide_changes,
  slide_grid,
  slide_points,
} from "../src/index.ts";

describe("array questions", () => {
  it("counts and finds equal elements", () => {
    expect(count_of([0, 2, 0, 1, 0], 0)).toBe(3);
    expect(nth_index_of([1, 0, 1, 0, 0], 0, 0)).toBe(1);
    expect(nth_index_of([1, 0, 1, 0, 0], 0, 2)).toBe(4);
    expect(nth_index_of([1, 0, 1, 0, 0], 0, 3)).toBe(-1);
    expect(arrays_equal([1, 2, 3], [1, 2, 3])).toBe(true);
    expect(arrays_equal([1, 2, 3], [1, 2, 4])).toBe(false);
  });

  it("measures lines through a cell without wrapping", () => {
    const lines = [2, 2, 2, 1, 2, 0, 1, 0, 2, 1, 0, 0];
    expect(line_through(lines, 4, 1, 2)).toBe(3);
    expect(line_through(lines, 4, 6, 1)).toBe(3);
    expect(line_through(lines, 4, 5, 2)).toBe(3);
    expect(line_through(lines, 4, 5, 1)).toBe(2);
    expect(line_through([0, 0, 0, 1, 1, 1, 0, 0], 4, 4, 1)).toBe(2);
  });
});

describe("2048 slides", () => {
  it("slide tiles to the edge and merge each pair once", () => {
    expect(slide_grid([1, 1, 1, 1], 4, "left")).toEqual([2, 2, 0, 0]);
    expect(slide_grid([0, 1, 0, 1], 4, "left")).toEqual([2, 0, 0, 0]);
    expect(slide_grid([2, 1, 1, 0], 4, "left")).toEqual([2, 2, 0, 0]);
    expect(slide_grid([1, 2, 3, 4], 4, "left")).toEqual([1, 2, 3, 4]);
    expect(slide_points([1, 1, 1, 1], 4, "left")).toBe(8);
  });

  it("move the board in every direction", () => {
    const board = [1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
    expect(slide_grid(board, 4, "left").slice(0, 4)).toEqual([2, 0, 0, 0]);
    expect(slide_grid(board, 4, "right").slice(0, 4)).toEqual([0, 0, 0, 2]);
    expect(slide_grid(board, 4, "up")[4]).toBe(2);
    expect(slide_grid(board, 4, "down")[12]).toBe(2);
    expect(slide_grid(board, 4, "down")[8]).toBe(1);
    expect(slide_points(board, 4, "left")).toBe(4);
    expect(slide_points(board, 4, "up")).toBe(0);
    expect(slide_changes(board, 4, "left")).toBe(true);
    expect(slide_changes([1, 0, 2, 0], 2, "up")).toBe(false);
  });

  it("see when no move is left", () => {
    expect(can_slide([1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1], 4)).toBe(false);
    expect(can_slide([1, 1, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1], 4)).toBe(true);
  });
});

describe("life", () => {
  it("turns a blinker and wraps at the edges", () => {
    const blinker = Array.from({ length: 25 }, (_, cell) => ([11, 12, 13].includes(cell) ? 1 : 0));
    const turned = life_step(blinker, 5, 5);
    expect([7, 12, 17].map((cell) => turned[cell])).toEqual([5, 5, 5]);
    expect(turned.filter((cell) => cell !== 0)).toHaveLength(3);
    const wrapped = Array.from({ length: 16 }, (_, cell) => ([3, 0, 1].includes(cell) ? 1 : 0));
    const next = life_step(wrapped, 4, 1);
    expect([0, 4, 12].map((cell) => next[cell])).toEqual([1, 1, 1]);
    expect(next.filter((cell) => cell !== 0)).toHaveLength(3);
  });
});
