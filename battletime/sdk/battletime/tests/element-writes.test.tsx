// Checks that writing one element of an array signal notifies and is seen by later reads.

import { computed, effect, signal } from "@preact/signals";
import { describe, expect, it } from "vitest";
import { persisted } from "battletime";

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

  it("go to the signal written through when two signals hold one array", () => {
    const board = signal([1, 2]);
    const trial = signal([0, 0]);
    trial.value = board.value;

    trial.value[0] = 7;

    expect(board.value[0]).toBe(1);
    expect(trial.value[0]).toBe(7);
  });

  it("include record fields, which notify and leave the old record unchanged", () => {
    const alarms = signal([
      { hour: 7, enabled: false },
      { hour: 8, enabled: false },
    ]);
    const second_before = alarms.peek()[1];
    const enabled_count = computed(() => alarms.value.filter((alarm) => alarm.enabled).length);

    alarms.value[1].enabled = true;

    expect(enabled_count.value).toBe(1);
    expect(alarms.value[1]).toEqual({ hour: 8, enabled: true });
    expect(second_before).toEqual({ hour: 8, enabled: false });
  });

  it("leave a copied array unshared", () => {
    const board = signal([1, 2]);
    const copy = signal(board.value);

    copy.value[0] = 7;

    expect(board.value[0]).toBe(1);
    expect(copy.value[0]).toBe(7);
  });
});
