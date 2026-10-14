import { describe, expect, it } from "vitest";
import { board, hits_wall, is_running, length, restart, steer, tick, STARTING_BOARD } from "./app.tsx";

/** The cells the snake covers, in board order. */
function snake_cells(): number[] {
  return board.value.flatMap((value, cell) => (value >= 1 && value <= 4 ? [cell] : []));
}

/** Starts a game on the starting board, with the food where the test wants it. */
function start_with_food(food: number) {
  restart();
  board.value = STARTING_BOARD.map((value, cell) => (cell === food ? 5 : value === 5 ? 0 : value));
}

describe("snake rules", () => {
  it("stops at every wall", () => {
    expect(hits_wall(0, -1)).toBe(true);
    expect(hits_wall(0, -16)).toBe(true);
    expect(hits_wall(15, 1)).toBe(true);
    expect(hits_wall(16, -1)).toBe(true);
    expect(hits_wall(255, 16)).toBe(true);
    expect(hits_wall(136, 1)).toBe(false);
    expect(hits_wall(239, 16)).toBe(false);
  });

  it("moves without growing, and grows by one for each meal with new food put down", () => {
    start_with_food(138);
    steer({ direction: "right" });
    tick();
    expect(snake_cells()).toEqual([137]);
    tick();
    expect(snake_cells()).toEqual([137, 138]);
    expect(length.value).toBe(2);
    expect(board.value.filter((value) => value === 5)).toHaveLength(1);
    tick();
    expect(snake_cells()).toEqual([138, 139]);
  });

  /** Grows the snake heading right from the start, one meal a tick, to the length given. */
  function grow_to(wanted: number) {
    start_with_food(137);
    steer({ direction: "right" });
    for (let meal = 1; meal < wanted; meal++) {
      tick();
      board.value = board.value.map((value) => (value === 5 ? 0 : value));
      if (meal < wanted - 1) board.value[137 + meal] = 5;
    }
  }

  it("can follow its own tail round a square", () => {
    grow_to(4);
    expect(snake_cells()).toEqual([136, 137, 138, 139]);
    for (const direction of ["down", "left", "up", "right"] as const) {
      steer({ direction });
      tick();
    }
    expect(is_running.value).toBe(true);
    expect(snake_cells()).toEqual([138, 139, 154, 155]);
  });

  it("ends the game when it runs into its body", () => {
    grow_to(5);
    for (const direction of ["down", "left"] as const) {
      steer({ direction });
      tick();
    }
    expect(is_running.value).toBe(true);
    steer({ direction: "up" });
    tick();
    expect(is_running.value).toBe(false);
  });

  it("ends the game at a wall", () => {
    start_with_food(0);
    steer({ direction: "up" });
    for (let step = 0; step < 9; step++) tick();
    expect(is_running.value).toBe(false);
  });
});
