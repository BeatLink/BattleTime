// Checks that writing one element of an array signal notifies and is seen by later reads.

import { computed, effect, signal } from "@preact/signals";
import { describe, expect, it } from "vitest";
import { persisted } from "neotime";

describe("element writes", () => {
  it("replace the array and notify readers", () => {
    const board = signal([0, 0, 0]);
    const total = computed(() => board.value.reduce((sum, cell) => sum + cell, 0));
    let runs = 0;
    effect(() => {
      board.value;
      runs++;
    });

    board.value[1] = 5;

    expect(total.value).toBe(5);
    expect(board.peek()).toEqual([0, 5, 0]);
    expect(runs).toBe(2);
  });

  it("are seen by later reads in the same handler", () => {
    const board = persisted("element_writes_board", [1, 2, 3]);

    board.value[0] = 9;
    const first = board.value[0];

    expect(first).toBe(9);
  });

  it("leave a copied array unshared", () => {
    const board = signal([1, 2]);
    const copy = signal(board.value);

    copy.value[0] = 7;

    expect(board.value[0]).toBe(1);
    expect(copy.value[0]).toBe(7);
  });
});
