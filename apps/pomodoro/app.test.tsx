import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE, SECOND } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, start_scheduler } from "neotime/preview";
import app, { ends_at, press_key, queue_text, rang_at, rounds_done, running_place, start, stop } from "./app.tsx";

let stop_scheduler = () => {};

const KEY_PLACES: Record<string, number> = {
  "1": 0, "2": 1, "3": 2, "4": 3, "5": 4, "6": 5, "7": 6, "8": 7, "9": 8, "0": 9, del: 10, then: 11, go: 12,
};

function press(...keys: string[]) {
  for (const key of keys) press_key({ value: KEY_PLACES[key] });
}

describe("pomodoro", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    stop();
    stop_scheduler();
    open(null);
  });

  it("types a queue with Then and deletes back through it", () => {
    press("del", "del", "del", "del");
    expect(queue_text.value).toBe("");
    press("2", "5", "then", "5");
    expect(queue_text.value).toBe("25,5");
    press("del");
    expect(queue_text.value).toBe("25,");
    press("del", "del");
    expect(queue_text.value).toBe("2");
  });

  it("runs each entry in turn and counts rounds", () => {
    press("del", "del", "del", "del", "2", "then", "1");
    start();
    advance_clock(2 * MINUTE);
    expect(rang_at.value).not.toBeNull();
    advance_clock(10 * SECOND);
    expect(rang_at.value).toBeNull();
    expect(running_place.value).toBe(1);
    expect(ends_at.value).not.toBeNull();
    advance_clock(MINUTE + 10 * SECOND);
    expect(running_place.value).toBe(0);
    expect(rounds_done.value).toBe(1);
  });
});
