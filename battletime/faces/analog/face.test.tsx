// Checks the date, the status icons and the second hand a long press switches off and on.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { is_charging, is_phone_connected, reset_persisted, unread_notifications } from "battletime";
import { icon_symbol, icons } from "battletime/icons";
import { freeze_clock, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

function hand_count(): number {
  return document.querySelectorAll("[data-widget=analog-clock] line").length;
}

function icon_symbols(): string[] {
  return [...document.querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent ?? "");
}

describe("analog", () => {
  beforeEach(() => {
    freeze_clock(new Date(2026, 9, 11, 10, 8, 30).getTime());
    const Face = face.screens.main;
    render(<Face />);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    unfreeze_clock();
    reset_persisted(face);
    is_charging.value = false;
    is_phone_connected.value = true;
    unread_notifications.value = 0;
  });

  it("shows the weekday over the day of the month", () => {
    expect(screen.getByText("SUN\n11", { normalizer: (text) => text })).toBeTruthy();
  });

  it("switches the second hand off and on with a long press", () => {
    vi.useFakeTimers();
    const screen_element = document.querySelector("[data-widget=screen]")!;
    expect(hand_count()).toBe(3);
    act(() => {
      fireEvent.pointerDown(screen_element);
      vi.advanceTimersByTime(500);
    });
    expect(hand_count()).toBe(2);
    act(() => {
      fireEvent.pointerUp(screen_element);
      fireEvent.pointerDown(screen_element);
      vi.advanceTimersByTime(500);
    });
    expect(hand_count()).toBe(3);
  });

  it("shows a charging bolt in place of the battery while charging", () => {
    expect(icon_symbols()).not.toContain(icon_symbol(icons.charging));
    act(() => { is_charging.value = true; });
    expect(icon_symbols()).toContain(icon_symbol(icons.charging));
  });

  it("shows the notification and connection icons only when they apply", () => {
    expect(icon_symbols()).toContain(icon_symbol(icons.bluetooth));
    expect(icon_symbols()).not.toContain(icon_symbol(icons.notification));
    act(() => {
      is_phone_connected.value = false;
      unread_notifications.value = 3;
    });
    expect(icon_symbols()).not.toContain(icon_symbol(icons.bluetooth));
    expect(icon_symbols()).toContain(icon_symbol(icons.notification));
  });
});
