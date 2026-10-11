import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE, SECOND } from "neotime";
import {
  advance_clock,
  effect_log,
  freeze_clock,
  install_apps,
  is_vibrating,
  open,
  start_scheduler,
} from "neotime/preview";
import app, { chosen_duration, dismiss, pause_or_resume, rang_at, remaining, start } from "./app.tsx";

let stop_scheduler = () => {};

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
