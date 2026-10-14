// Checks the two dials a long press switches between, and the second hand a double tap hides.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reset_persisted, steps_today, unread_notifications } from "neotime";
import { icon_symbol, icons } from "neotime/icons";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

function screen_element(): HTMLElement {
  return document.querySelector("[data-widget=screen]") as HTMLElement;
}

function long_press() {
  act(() => {
    fireEvent.pointerDown(screen_element());
    vi.advanceTimersByTime(500);
  });
  act(() => { fireEvent.pointerUp(screen_element()); });
}

function hand_count(): number {
  return document.querySelectorAll("[data-widget=analog-clock] line").length;
}

describe("lite analog", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    freeze_clock(new Date(2026, 9, 11, 10, 8, 30).getTime());
    const Face = face.screens.main;
    render(<Face />);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    unfreeze_clock();
    reset_persisted(face);
    steps_today.value = 0;
    unread_notifications.value = 0;
  });

  it("draws the classic dial with the weekday and day", () => {
    expect(screen.getByText("PineTime")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
    expect(screen.getByText("SUN\n11", { normalizer: (text) => text })).toBeTruthy();
    expect(document.querySelector("[data-widget=scale]")).not.toBeNull();
  });

  it("switches to the rays dial with the step count on a long press, and back", () => {
    act(() => { steps_today.value = 2345; });
    long_press();
    expect(screen.queryByText("PineTime")).toBeNull();
    expect(screen.getByText("STEPS\n2345", { normalizer: (text) => text })).toBeTruthy();
    expect(document.querySelector("[data-widget=analog-clock] line")?.getAttribute("stroke")).toBe("#ffff00");
    long_press();
    expect(screen.getByText("PineTime")).toBeTruthy();
  });

  it("hides the second hand on a double tap and shows it on the next", () => {
    expect(hand_count()).toBe(3);
    act(() => { fireEvent.dblClick(screen_element()); });
    expect(hand_count()).toBe(2);
    act(() => { fireEvent.dblClick(screen_element()); });
    expect(hand_count()).toBe(3);
  });

  it("shows the notification icon while there are unread notifications", () => {
    expect(screen.queryByText(icon_symbol(icons.notification))).toBeNull();
    act(() => { unread_notifications.value = 1; });
    expect(screen.getByText(icon_symbol(icons.notification))).toBeTruthy();
  });
});
