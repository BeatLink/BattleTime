import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE, SECOND } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, start_scheduler, swipe } from "neotime/preview";
import app, {
  ends_at,
  interval,
  long_break_minutes,
  rang_at,
  short_break_minutes,
  start,
  stop,
  work_minutes,
  works_done,
  works_per_long_break,
} from "./app.tsx";

let stop_scheduler = () => {};

const WORK = 0;
const SHORT_BREAK = 1;
const LONG_BREAK = 2;
const BUZZ = 10 * SECOND;

/** Runs the interval on now to its end and through the buzz after it. */
function finish_interval(minutes: number) {
  advance_clock(minutes * MINUTE);
  advance_clock(BUZZ);
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

  it("takes work and short breaks in turn, with a long break after every Nth work", () => {
    work_minutes.value = 3;
    short_break_minutes.value = 1;
    long_break_minutes.value = 2;
    works_per_long_break.value = 2;
    start();
    expect(interval.value).toBe(WORK);
    finish_interval(3);
    expect(interval.value).toBe(SHORT_BREAK);
    expect(works_done.value).toBe(1);
    finish_interval(1);
    expect(interval.value).toBe(WORK);
    finish_interval(3);
    expect(interval.value).toBe(LONG_BREAK);
    expect(works_done.value).toBe(0);
    finish_interval(2);
    expect(interval.value).toBe(WORK);
    finish_interval(3);
    expect(interval.value).toBe(SHORT_BREAK);
  });

  it("buzzes between intervals and starts the next after ten seconds", () => {
    work_minutes.value = 1;
    start();
    advance_clock(MINUTE);
    expect(rang_at.value).not.toBeNull();
    expect(ends_at.value).toBeNull();
    advance_clock(BUZZ - SECOND);
    expect(rang_at.value).not.toBeNull();
    advance_clock(SECOND);
    expect(rang_at.value).toBeNull();
    expect(ends_at.value).not.toBeNull();
  });

  it("uses a changed length from the next interval that needs it", () => {
    work_minutes.value = 2;
    start();
    work_minutes.value = 10;
    advance_clock(2 * MINUTE);
    expect(rang_at.value).not.toBeNull();
    advance_clock(BUZZ);
    finish_interval(short_break_minutes.value);
    expect(interval.value).toBe(WORK);
    advance_clock(9 * MINUTE);
    expect(ends_at.value).not.toBeNull();
    expect(rang_at.value).toBeNull();
  });

  it("opens on the countdown again after leaving it on another page", () => {
    render(<OpenScreen />);
    act(() => swipe("right"));
    expect(screen.queryByText("Start")).toBeNull();
    cleanup();
    open(null);
    open(app);
    render(<OpenScreen />);
    expect(screen.getByText("Start")).toBeTruthy();
    cleanup();
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

  it("opens on the countdown, with the lengths below the work time", () => {
    expect(screen.getByText("Start")).toBeTruthy();
    expect(screen.getByText("25:00")).toBeTruthy();
    expect(screen.getByText("25 / 5 / 15 min")).toBeTruthy();
  });

  it("reaches the long break, short break and work pages by swiping right", () => {
    act(() => swipe("right"));
    expect(screen.getByText("After 4 work sessions")).toBeTruthy();
    act(() => swipe("right"));
    expect(screen.getByText("Short break")).toBeTruthy();
    act(() => swipe("right"));
    expect(screen.getByText("Work")).toBeTruthy();
  });

  it("counts down from Start, adds a minute on +1, and names the interval and the round", () => {
    tap("Start");
    expect(screen.getByText("25:00")).toBeTruthy();
    expect(screen.getByText("Work")).toBeTruthy();
    expect(screen.getByText("1 of 4")).toBeTruthy();
    tap("+1");
    expect(screen.getByText("26:00")).toBeTruthy();
    act(() => advance_clock(MINUTE));
    expect(screen.getByText("25:00")).toBeTruthy();
  });

  it("names the next interval while buzzing, and counts the break after the work it follows", () => {
    tap("Start");
    act(() => advance_clock(25 * MINUTE));
    expect(screen.getByText("Short break next")).toBeTruthy();
    expect(screen.queryByText("+1")).toBeNull();
    act(() => advance_clock(BUZZ));
    expect(screen.getByText("Short break")).toBeTruthy();
    expect(screen.getByText("1 of 4")).toBeTruthy();
    expect(screen.getByText("05:00")).toBeTruthy();
  });

  it("sets each length with its page's - and + buttons", () => {
    act(() => swipe("right"));
    const [long_minus, works_minus] = screen.getAllByText("-");
    const [long_plus, works_plus] = screen.getAllByText("+");
    act(() => long_plus.click());
    expect(long_break_minutes.value).toBe(16);
    act(() => works_minus.click());
    expect(works_per_long_break.value).toBe(3);
    expect(screen.getByText("After 3 work sessions")).toBeTruthy();
    act(() => long_minus.click());
    act(() => works_plus.click());
    expect(long_break_minutes.value).toBe(15);
    expect(works_per_long_break.value).toBe(4);
    act(() => swipe("right"));
    act(() => screen.getByText("-").click());
    expect(short_break_minutes.value).toBe(4);
    act(() => swipe("right"));
    act(() => screen.getByText("+").click());
    expect(work_minutes.value).toBe(26);
  });

  it("counts the long break as the end of the round", () => {
    works_per_long_break.value = 2;
    tap("Start");
    act(() => advance_clock(25 * MINUTE + BUZZ));
    act(() => advance_clock(5 * MINUTE + BUZZ));
    act(() => advance_clock(25 * MINUTE + BUZZ));
    expect(screen.getByText("Long break")).toBeTruthy();
    expect(screen.getByText("2 of 2")).toBeTruthy();
    expect(screen.getByText("15:00")).toBeTruthy();
  });

  it("goes back to Start on Stop", () => {
    tap("Start");
    tap("Stop");
    expect(screen.getByText("Start")).toBeTruthy();
    expect(ends_at.value).toBeNull();
    expect(interval.value).toBe(WORK);
  });
});
