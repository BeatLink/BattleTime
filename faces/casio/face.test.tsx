// Checks the week numbers against strftime's %V and %U, and the day-of-year line around the new year.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { settings } from "neotime";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import { sunday_week_of } from "neotime/lib";
import face, { week_number_of } from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

function local_noon(year: number, month: number, day: number): number {
  return new Date(year, month - 1, day, 12, 0, 0).getTime();
}

/** Every label's text on the face as drawn on a given day. */
function label_texts_on(year: number, month: number, day: number): string[] {
  freeze_clock(local_noon(year, month, day));
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

// ============================================================================
// Tests
// ============================================================================

describe("casio", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    settings.uses_24_hour_clock.value = true;
  });

  it("counts ISO weeks across year ends on a 24 hour clock", () => {
    expect(week_number_of(local_noon(2021, 1, 1), true)).toBe(53);
    expect(week_number_of(local_noon(2026, 1, 1), true)).toBe(1);
    expect(week_number_of(local_noon(2024, 12, 30), true)).toBe(1);
    expect(week_number_of(local_noon(2026, 10, 11), true)).toBe(41);
  });

  it("counts weeks from the first Sunday on a 12 hour clock", () => {
    expect(sunday_week_of(local_noon(2026, 1, 3))).toBe(0);
    expect(sunday_week_of(local_noon(2026, 1, 4))).toBe(1);
    expect(sunday_week_of(local_noon(2023, 1, 1))).toBe(1);
    expect(week_number_of(local_noon(2026, 10, 11), false)).toBe(41);
  });

  it("shows the week, weekday and day of the year with the days left", () => {
    const texts = label_texts_on(2024, 12, 31);
    expect(texts).toContain("WK01");
    expect(texts).toContain("TUE");
    expect(texts).toContain("366-  0");
    expect(texts).toContain(" 31-12");
  });

  it("puts the month first and starts weeks on Sunday on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    const texts = label_texts_on(2026, 1, 3);
    expect(texts).toContain("WK00");
    expect(texts).toContain("  1- 3");
    expect(texts).toContain("12:00");
    expect(texts).toContain("P");
  });
});
