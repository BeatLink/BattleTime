import { describe, expect, it } from "vitest";
import { board, move, start_game } from "./app.tsx";

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
});
