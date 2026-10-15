import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE, SECOND } from "battletime";
import {
  advance_clock,
  effect_log,
  freeze_clock,
  install_apps,
  is_vibrating,
  open,
  open_app,
  OpenScreen,
  press_button,
  start_scheduler,
} from "battletime/preview";
import app, { chosen_duration, dismiss, pause_or_resume, rang_at, remaining, start } from "./app.tsx";

let stop_scheduler = () => {};

function buttons(): HTMLElement[] {
  return screen.getAllByRole("button");
}

function tap(element: HTMLElement) {
  act(() => element.click());
}

describe("timer", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    dismiss();
    stop_scheduler();
    open(null);
  });

  it("rings when the time is up and stops on dismiss", () => {
    chosen_duration.value = MINUTE;
    start();
    advance_clock(30 * SECOND);
    expect(remaining.value).toBe(30 * SECOND);
    advance_clock(30 * SECOND);
    expect(rang_at.value).not.toBeNull();
    expect(is_vibrating.value).toBe(true);
    expect(effect_log.value.some((effect) => effect.kind === "wake_screen")).toBe(true);
    dismiss();
    expect(rang_at.value).toBeNull();
    expect(is_vibrating.value).toBe(false);
  });

  it("holds the time left while paused", () => {
    chosen_duration.value = MINUTE;
    start();
    advance_clock(20 * SECOND);
    pause_or_resume();
    advance_clock(5 * MINUTE);
    expect(remaining.value).toBe(40 * SECOND);
    expect(rang_at.value).toBeNull();
    pause_or_resume();
    advance_clock(40 * SECOND);
    expect(rang_at.value).not.toBeNull();
  });

  it("falls silent by itself after a minute of ringing", () => {
    chosen_duration.value = 10 * SECOND;
    start();
    advance_clock(10 * SECOND);
    expect(is_vibrating.value).toBe(true);
    advance_clock(MINUTE);
    expect(is_vibrating.value).toBe(false);
    expect(rang_at.value).not.toBeNull();
  });
});

describe("timer screen", () => {
  beforeEach(() => {
    freeze_clock(Date.UTC(2026, 9, 11, 8, 0, 0));
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    cleanup();
    dismiss();
    stop_scheduler();
    open(null);
  });

  it("counts down from the chosen time once started", () => {
    render(<OpenScreen />);
    tap(buttons()[0]);
    expect(screen.getByText("10:00")).toBeTruthy();
    act(() => advance_clock(61 * SECOND));
    expect(screen.getByText("08:59")).toBeTruthy();
  });

  it("offers a reset while paused, which goes back to choosing", () => {
    render(<OpenScreen />);
    tap(buttons()[0]);
    tap(buttons()[0]);
    expect(buttons()).toHaveLength(2);
    tap(buttons()[0]);
    expect(screen.queryByText("10:00")).toBeNull();
    expect(document.querySelectorAll("[data-widget=roller]")).toHaveLength(2);
  });

  it("resumes from the paused time on the play button", () => {
    render(<OpenScreen />);
    tap(buttons()[0]);
    act(() => advance_clock(30 * SECOND));
    tap(buttons()[0]);
    act(() => advance_clock(5 * MINUTE));
    expect(screen.getByText("09:30")).toBeTruthy();
    tap(buttons()[1]);
    act(() => advance_clock(10 * SECOND));
    expect(screen.getByText("09:20")).toBeTruthy();
  });

  it("shows hours for a run of a hundred minutes or more", () => {
    chosen_duration.value = 100 * MINUTE;
    render(<OpenScreen />);
    tap(buttons()[0]);
    expect(screen.getByText("1:40:00")).toBeTruthy();
  });

  it("cannot start from zero", () => {
    chosen_duration.value = 0;
    render(<OpenScreen />);
    expect((buttons()[0] as HTMLButtonElement).disabled).toBe(true);
    start();
    expect(remaining.value).toBe(0);
  });

  it("counts the time since the end while ringing and stops on a tap anywhere", () => {
    chosen_duration.value = 10 * SECOND;
    render(<OpenScreen />);
    tap(buttons()[0]);
    act(() => advance_clock(15 * SECOND));
    expect(screen.getByText("Time's up")).toBeTruthy();
    expect(screen.getByText("+00:05")).toBeTruthy();
    tap(document.querySelector("[data-widget=screen]") as HTMLElement);
    expect(rang_at.value).toBeNull();
    expect(screen.queryByText("Time's up")).toBeNull();
  });

  it("stops the ring with the Dismiss button", () => {
    chosen_duration.value = 10 * SECOND;
    render(<OpenScreen />);
    tap(buttons()[0]);
    act(() => advance_clock(10 * SECOND));
    tap(screen.getByText("Dismiss"));
    expect(rang_at.value).toBeNull();
  });

  it("stops the ring when the side button leaves the app", () => {
    chosen_duration.value = 10 * SECOND;
    render(<OpenScreen />);
    tap(buttons()[0]);
    act(() => advance_clock(10 * SECOND));
    act(() => press_button());
    expect(rang_at.value).toBeNull();
    expect(open_app.value).toBeNull();
  });
});
