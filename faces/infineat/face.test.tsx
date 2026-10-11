// Checks that the pine logo's fill follows the battery, fills up again and again while charging, and takes the cover's colours.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { battery_percent, is_charging, reset_persisted } from "neotime";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

/** The bar over the logo, from the face as drawn at a given second. */
function cover_at(second: number): Element | null {
  freeze_clock(new Date(2026, 9, 11, 10, 30, second).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return container.querySelector("polyline[stroke-width='24']");
}

/** The bottom of the bar over the logo at a given second. */
function cover_bottom_at(second: number): number {
  const points = cover_at(second)?.getAttribute("points") ?? "";
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
    reset_persisted(face);
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

  it("draws the bar in the palette's fifth colour with the side cover and black without it", () => {
    expect(cover_at(0)?.getAttribute("stroke")).toBe("#ffffff");
    cleanup();
    const Settings = face.screens.settings;
    const { getByText } = render(<Settings />);
    getByText("Side cover: On").click();
    cleanup();
    expect(cover_at(0)?.getAttribute("stroke")).toBe("#000000");
  });
});
