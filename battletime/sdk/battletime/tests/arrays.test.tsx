import { describe, expect, it } from "vitest";
import {
  arrays_equal,
  count_inversions,
  count_of,
  format_fixed,
  line_through,
  nth_index_of,
  push_shift,
  shuffled,
  unshift_pop,
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

  it("move a history along with a new value", () => {
    expect(push_shift([1, 2, 3], 9)).toEqual([2, 3, 9]);
    expect(unshift_pop([1, 2, 3], 9)).toEqual([9, 1, 2]);
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

describe("numbers and puzzles", () => {
  it("formats fixed-point numbers", () => {
    expect(format_fixed(1250, 3)).toBe("1.250");
    expect(format_fixed(1250, 3, true)).toBe("1.25");
    expect(format_fixed(-50, 2)).toBe("-0.50");
    expect(format_fixed(3_000_000, 6, true)).toBe("3");
    expect(format_fixed(7, 0)).toBe("7");
  });

  it("counts inversions and shuffles only the places asked for", () => {
    expect(count_inversions([3, 1, 2])).toBe(2);
    const result = shuffled([1, 2, 3, 4, 5, 6], 1, 4);
    expect([result[0], result[5]]).toEqual([1, 6]);
    expect(result.slice(1, 5).sort()).toEqual([2, 3, 4, 5]);
  });
});
