import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, MINUTE } from "neotime";
import { advance_clock, freeze_clock, install_apps, is_vibrating, open, screen_stack, start_scheduler } from "neotime/preview";
import app, {
  alarm_enabled,
  dismiss,
  editing_days,
  editing_time,
  next_ring_at,
  open_new_alarm,
  rang_at,
  save_alarm,
  snooze,
  snoozed_until,
} from "./app.tsx";

let stop_scheduler = () => {};

/** Saturday 10 October 2026 at 06:00 local time. */
const SATURDAY_MORNING = new Date(2026, 9, 10, 6, 0, 0).getTime();

function add_alarm(hour: number, minute: number, days: number) {
  open_new_alarm();
  editing_time.value = { hour, minute };
  editing_days.value = days;
  save_alarm();
}

describe("alarm", () => {
  beforeEach(() => {
    freeze_clock(SATURDAY_MORNING);
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    stop_scheduler();
    open(null);
  });

  it("follows the soonest alarm and skips days a repeat leaves out", () => {
    add_alarm(7, 0, 0b0011111);
    expect(next_ring_at.value).toBe(new Date(2026, 9, 12, 7, 0, 0).getTime());
    add_alarm(6, 30, 0);
    expect(next_ring_at.value).toBe(new Date(2026, 9, 10, 6, 30, 0).getTime());
  });

  it("rings, turns a one-off alarm off, and snoozes for ten minutes", () => {
    add_alarm(6, 30, 0);
    advance_clock(30 * MINUTE);
    expect(rang_at.value).not.toBeNull();
    expect(is_vibrating.value).toBe(true);
    expect(screen_stack.value.at(-1)).toBe("ring");
    expect(alarm_enabled.value[0]).toBe(false);

    snooze();
    expect(is_vibrating.value).toBe(false);
    expect(snoozed_until.value).toBe(new Date(2026, 9, 10, 6, 40, 0).getTime());
    expect(next_ring_at.value).toBe(snoozed_until.value);

    advance_clock(10 * MINUTE);
    expect(rang_at.value).not.toBeNull();
    expect(snoozed_until.value).toBeNull();
    dismiss();
    expect(next_ring_at.value).toBeNull();
  });

  it("stops ringing by itself after a minute", () => {
    add_alarm(6, 1, 0);
    advance_clock(MINUTE);
    expect(is_vibrating.value).toBe(true);
    advance_clock(MINUTE);
    expect(is_vibrating.value).toBe(false);
    expect(rang_at.value).toBeNull();
  });
});
