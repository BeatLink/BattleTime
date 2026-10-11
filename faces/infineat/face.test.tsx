// Checks that the pine logo's fill follows the battery, and fills up again and again while charging.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { battery_percent, is_charging } from "neotime";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

/** The bottom of the dark bar over the logo, read from the face as drawn at a given second. */
function cover_bottom_at(second: number): number {
  freeze_clock(new Date(2026, 9, 11, 10, 30, second).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  const cover = container.querySelector("polyline[stroke-width='26']");
  const points = cover?.getAttribute("points") ?? "";
  return Number(points.split(" ")[1].split(",")[1]);
}

// ============================================================================
// Tests
// ============================================================================

describe("infineat", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
    battery_percent.value = 80;
    is_charging.value = false;
  });

  it("hides the drained part of the logo off the charger, whatever the second", () => {
    battery_percent.value = 80;
    expect(cover_bottom_at(2)).toBe(112);
    cleanup();
    expect(cover_bottom_at(4)).toBe(112);
  });

  it("fills the logo a quarter of the way more each second while charging", () => {
    battery_percent.value = 80;
    is_charging.value = true;
    expect(cover_bottom_at(0)).toBe(112);
    cleanup();
    expect(cover_bottom_at(2)).toBe(109);
    cleanup();
    expect(cover_bottom_at(4)).toBe(106);
  });
});
