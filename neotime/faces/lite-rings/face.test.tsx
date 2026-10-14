// Checks the time and date on both clock types, and the rings that fill with the hour, minute and second.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { settings, unread_notifications } from "neotime";
import { icon_symbol, icons } from "neotime/icons";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

function draw_at(hour: number, minute: number, second = 0): Element {
  freeze_clock(new Date(2026, 9, 11, hour, minute, second).getTime());
  const Face = face.screens.main;
  return render(<Face />).container;
}

function labels(container: Element): string[] {
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

/** How many rings have a filled part drawn over their track. */
function filled_rings(container: Element): number {
  return [...container.querySelectorAll("[data-widget=arc] svg")].filter((ring) => ring.querySelectorAll("path").length === 2)
    .length;
}

describe("lite rings", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
    unread_notifications.value = 0;
  });

  it("shows the hour, the minute and the date day first on a 24 hour clock", () => {
    const shown = labels(draw_at(8, 5));
    expect(shown).toEqual(["08", "Sun, 11 October", "05"]);
  });

  it("drops the leading zero, adds AM or PM and puts the month first on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    expect(labels(draw_at(20, 5))).toEqual(["8", "PM", "Sun, October 11", "05"]);
    cleanup();
    expect(labels(draw_at(0, 5))).toEqual(["12", "AM", "Sun, October 11", "05"]);
  });

  it("fills a ring for each of the hour, minute and second once they pass zero", () => {
    expect(filled_rings(draw_at(0, 0, 0))).toBe(0);
    cleanup();
    expect(filled_rings(draw_at(8, 5, 30))).toBe(3);
  });

  it("shows the notification icon while there are unread notifications", () => {
    unread_notifications.value = 1;
    expect([...draw_at(12, 0).querySelectorAll("[data-widget=icon]")].map((icon) => icon.textContent)).toContain(
      icon_symbol(icons.notification),
    );
  });
});
