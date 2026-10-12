import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, settings, MINUTE } from "neotime";
import {
  advance_clock,
  freeze_clock,
  install_apps,
  is_vibrating,
  open,
  open_app,
  OpenScreen,
  press_button,
  screen_stack,
  start_scheduler,
} from "neotime/preview";
import app, {
  alarms,
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

const DAY_LENGTH = 24 * 60 * MINUTE;

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
    expect(alarms.value[0].enabled).toBe(false);

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

function buttons(): HTMLElement[] {
  return screen.getAllByRole("button");
}

function tap(element: Element) {
  act(() => (element as HTMLElement).click());
}

describe("alarm screens", () => {
  beforeEach(() => {
    freeze_clock(SATURDAY_MORNING);
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    cleanup();
    settings.uses_24_hour_clock.value = true;
    stop_scheduler();
    open(null);
  });

  it("adds an alarm through the time and day screens", () => {
    render(<OpenScreen />);
    expect(screen.getByText("No alarm set")).toBeTruthy();
    tap(buttons()[0]);
    expect(screen_stack.value.at(-1)).toBe("edit_time");
    tap(document.querySelector("[data-widget=roller] [data-roller-offset='-1']")!);
    tap(buttons()[1]);
    expect(screen.getByText("07:00")).toBeTruthy();
    tap(screen.getByText("M"));
    tap(screen.getByText("W"));
    expect(screen.getByText("M-W----")).toBeTruthy();
    tap(buttons().at(-1)!);
    expect(screen_stack.value).toEqual(["main"]);
    expect(screen.getByText("07:00")).toBeTruthy();
    expect(screen.getByText("M-W----")).toBeTruthy();
    expect(screen.getByText("Mon 07:00, in 49:00")).toBeTruthy();
  });

  it("cycles the repeat button through the common patterns", () => {
    render(<OpenScreen />);
    tap(buttons()[0]);
    tap(buttons()[1]);
    const names: string[] = [];
    for (let press = 0; press < 5; press++) {
      names.push(buttons()[7].textContent ?? "");
      tap(buttons()[7]);
    }
    expect(names).toEqual(["Once", "Every day", "Weekdays", "Weekends", "Once"]);
    tap(screen.getAllByText("T")[0]);
    tap(buttons()[7]);
    expect(buttons()[7].textContent).toBe("Once");
  });

  it("adds nothing when the new alarm is cancelled or closed", () => {
    render(<OpenScreen />);
    tap(buttons()[0]);
    tap(buttons()[0]);
    expect(screen_stack.value).toEqual(["main"]);
    tap(buttons()[0]);
    tap(buttons()[1]);
    tap(screen.getByText("Cancel"));
    expect(screen_stack.value).toEqual(["main"]);
    expect(screen.getByText("No alarm set")).toBeTruthy();
    expect(alarms.value[0].enabled).toBe(false);
  });

  it("switches an alarm off and on from its row", () => {
    add_alarm(7, 0, 0);
    open(app);
    render(<OpenScreen />);
    tap(screen.getByRole("switch"));
    expect(screen.getByText("No alarm set")).toBeTruthy();
    tap(screen.getByRole("switch"));
    expect(next_ring_at.value).toBe(new Date(2026, 9, 10, 7, 0, 0).getTime());
  });

  it("edits and deletes an alarm from its row", () => {
    add_alarm(7, 0, 0);
    add_alarm(9, 15, 0);
    open(app);
    render(<OpenScreen />);
    tap(screen.getByText("07:00"));
    tap(buttons()[1]);
    tap(screen.getByText("S", { selector: "button:nth-child(6)" }));
    tap(buttons().at(-1)!);
    expect(screen.getByText("-----S-")).toBeTruthy();
    tap(screen.getByText("07:00"));
    tap(buttons()[1]);
    tap(screen.getByText("Delete"));
    expect(screen.queryByText("07:00")).toBeNull();
    expect(screen.getByText("09:15")).toBeTruthy();
  });

  it("shows times on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    add_alarm(0, 5, 0);
    add_alarm(13, 30, 0);
    open(app);
    render(<OpenScreen />);
    expect(screen.getByText("12:05 AM")).toBeTruthy();
    expect(screen.getByText("01:30 PM")).toBeTruthy();
  });

  it("takes no more than twelve alarms", () => {
    for (let hour = 0; hour < 12; hour++) add_alarm(hour + 7, 0, 0);
    open(app);
    render(<OpenScreen />);
    expect(screen.getAllByRole("switch")).toHaveLength(12);
    expect(screen.queryAllByRole("button")).toHaveLength(0);
    open_new_alarm();
    expect(screen_stack.value).toEqual(["main"]);
  });

  it("rings with the time and stops on the side button", () => {
    add_alarm(6, 1, 0);
    open(app);
    render(<OpenScreen />);
    act(() => advance_clock(MINUTE));
    expect(screen.getByText("Alarm")).toBeTruthy();
    expect(screen.getByText("06:01")).toBeTruthy();
    act(() => press_button());
    expect(rang_at.value).toBeNull();
    expect(open_app.value).toBeNull();
  });

  it("snoozes from the Snooze button", () => {
    add_alarm(6, 1, 0);
    open(app);
    render(<OpenScreen />);
    act(() => advance_clock(MINUTE));
    tap(screen.getByText("Snooze"));
    expect(snoozed_until.value).toBe(new Date(2026, 9, 10, 6, 11, 0).getTime());
  });

  it("drops a snooze when its alarm is switched off, saved again or deleted", () => {
    add_alarm(6, 1, 0b1111111);
    add_alarm(6, 1, 0b1111111);
    advance_clock(MINUTE);
    snooze();
    open(app);
    render(<OpenScreen />);
    tap(screen.getAllByRole("switch")[1]);
    expect(snoozed_until.value).not.toBeNull();
    tap(screen.getAllByRole("switch")[0]);
    expect(snoozed_until.value).toBeNull();

    tap(screen.getAllByRole("switch")[0]);
    advance_clock(DAY_LENGTH);
    snooze();
    open(app);
    tap(screen.getAllByText("06:01")[0]);
    tap(buttons()[1]);
    tap(buttons().at(-1)!);
    expect(snoozed_until.value).toBeNull();
  });

  it("keeps a later alarm's snooze when an earlier alarm is deleted, and drops it with its own", () => {
    add_alarm(5, 0, 0b1111111);
    add_alarm(6, 1, 0b1111111);
    advance_clock(MINUTE);
    snooze();
    open(app);
    render(<OpenScreen />);
    tap(screen.getByText("05:00"));
    tap(buttons()[1]);
    tap(screen.getByText("Delete"));
    expect(snoozed_until.value).not.toBeNull();
    tap(screen.getByText("06:01"));
    tap(buttons()[1]);
    tap(screen.getByText("Delete"));
    expect(snoozed_until.value).toBeNull();
  });
});
