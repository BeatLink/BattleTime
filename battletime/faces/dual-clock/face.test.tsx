// Checks the hours over the minutes on both clock types and the battery colours of the status bar.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { battery_percent, is_phone_connected, settings, unread_notifications } from "battletime";
import { icon_symbol, icons } from "battletime/icons";
import { freeze_clock, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

function draw_at(hour: number, minute: number): Element {
  freeze_clock(new Date(2026, 9, 11, hour, minute, 0).getTime());
  const Face = face.screens.main;
  return render(<Face />).container;
}

function big_digits(container: Element): string[] {
  return [...container.querySelectorAll<HTMLElement>("[data-widget=label]")]
    .filter((label) => label.style.fontSize === "76px")
    .map((label) => label.textContent ?? "");
}

/** The colour the battery icon's outline is drawn in. */
function battery_outline(container: Element): string {
  return container.querySelector<HTMLElement>("[data-widget=battery-body]")!.style.border;
}

describe("dual clock", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
    battery_percent.value = 80;
    is_phone_connected.value = true;
    unread_notifications.value = 0;
  });

  it("puts the hours over the minutes, both with two digits", () => {
    expect(big_digits(draw_at(8, 5))).toEqual(["08", "05"]);
  });

  it("counts hours from 1 to 12 on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    expect(big_digits(draw_at(20, 5))).toEqual(["08", "05"]);
    cleanup();
    expect(big_digits(draw_at(0, 5))).toEqual(["12", "05"]);
  });

  it("colours the battery green, then yellow, then red as it runs down", () => {
    battery_percent.value = 80;
    expect(battery_outline(draw_at(12, 0))).toContain("rgb(0, 192, 0)");
    cleanup();
    battery_percent.value = 40;
    expect(battery_outline(draw_at(12, 0))).toContain("rgb(224, 192, 0)");
    cleanup();
    battery_percent.value = 10;
    expect(battery_outline(draw_at(12, 0))).toContain("rgb(224, 0, 0)");
  });

  it("shows the notification and connection icons only when they apply", () => {
    is_phone_connected.value = false;
    let symbols = [...draw_at(12, 0).querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent);
    expect(symbols).toEqual([]);
    cleanup();
    is_phone_connected.value = true;
    unread_notifications.value = 2;
    symbols = [...draw_at(12, 0).querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent);
    expect(symbols).toEqual([icon_symbol(icons.notification), icon_symbol(icons.bluetooth)]);
  });
});
