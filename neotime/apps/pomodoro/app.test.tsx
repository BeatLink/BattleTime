import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE, SECOND } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, start_scheduler, swipe } from "neotime/preview";
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

function tap(text: string) {
  act(() => screen.getByText(text).click());
}

describe("pomodoro screen", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop();
    stop_scheduler();
    open(null);
  });

  it("changes the buzz length on up and down swipes, wrapping between 1 and 15 seconds", () => {
    expect(screen.getByText("V10")).toBeTruthy();
    act(() => swipe("up"));
    expect(screen.getByText("V11")).toBeTruthy();
    for (let step = 0; step < 4; step++) act(() => swipe("up"));
    expect(screen.getByText("V15")).toBeTruthy();
    act(() => swipe("up"));
    expect(screen.getByText("V1")).toBeTruthy();
    act(() => swipe("down"));
    expect(screen.getByText("V15")).toBeTruthy();
    act(() => swipe("down"));
    expect(screen.getByText("V14")).toBeTruthy();
  });

  it("steps through the preset queues on left and right swipes", () => {
    act(() => swipe("right"));
    expect(screen.getByText("50,10")).toBeTruthy();
    act(() => swipe("right"));
    expect(screen.getByText("25,5,25,5,25,15")).toBeTruthy();
    act(() => swipe("right"));
    expect(screen.getByText("25,5")).toBeTruthy();
    act(() => swipe("left"));
    expect(screen.getByText("25,5,25,5,25,15")).toBeTruthy();
  });

  it("counts down from Go, adds a minute on +1 and ignores swipes while running", () => {
    tap("Go");
    expect(screen.getByText("25:00")).toBeTruthy();
    expect(screen.getByText("Timer #1/2  (0)", { normalizer: (text) => text })).toBeTruthy();
    tap("+1");
    expect(screen.getByText("26:00")).toBeTruthy();
    act(() => swipe("right"));
    act(() => advance_clock(MINUTE));
    expect(screen.getByText("25:00")).toBeTruthy();
    expect(queue_text.value).toBe("25,5");
  });

  it("shows the buzz between entries and goes back to the keypad on Stop", () => {
    press("del", "del", "del", "del", "1");
    tap("Go");
    act(() => advance_clock(MINUTE));
    expect(screen.getByText("Pomodoro")).toBeTruthy();
    expect(screen.queryByText("+1")).toBeNull();
    tap("Stop");
    expect(screen.getByText("Go")).toBeTruthy();
    expect(rang_at.value).toBeNull();
  });

  it("keeps entries between 1 and 999 minutes and the queue to six", () => {
    press("del", "del", "del", "del", "0");
    expect(queue_text.value).toBe("");
    press("then");
    expect(queue_text.value).toBe("");
    press("9", "9", "9", "9");
    expect(queue_text.value).toBe("999");
    press("then", "0");
    expect(queue_text.value).toBe("999,");
    press("1", "then", "2", "then", "3", "then", "4", "then", "5", "then");
    expect(queue_text.value).toBe("999,1,2,3,4,5");
    press("6");
    expect(queue_text.value).toBe("999,1,2,3,4,56");
    press("del", "del");
    expect(queue_text.value).toBe("999,1,2,3,4,");
    press("del");
    expect(queue_text.value).toBe("999,1,2,3,4");
  });

  it("will not start on an empty queue or one waiting for its next entry", () => {
    press("then", "go");
    expect(ends_at.value).toBeNull();
    press("del", "del", "del", "del", "del", "go");
    expect(queue_text.value).toBe("");
    expect(ends_at.value).toBeNull();
  });

  it("stops by itself after 99 rounds", () => {
    press("del", "del", "del", "del", "1");
    for (let step = 0; step < 9; step++) act(() => swipe("down"));
    expect(screen.getByText("V1")).toBeTruthy();
    start();
    cleanup();
    advance_clock(98 * (MINUTE + SECOND) + MINUTE);
    expect(rounds_done.value).toBe(98);
    expect(rang_at.value).not.toBeNull();
    advance_clock(SECOND);
    expect(ends_at.value).toBeNull();
    expect(rang_at.value).toBeNull();
    expect(rounds_done.value).toBe(0);
  });
});
