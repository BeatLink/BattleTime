// Checks that the squares add up to the hour and the minutes in fives.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

// ============================================================================
// Helpers
// ============================================================================

const WHITE = "rgb(255, 255, 255)";
const RED = "rgb(255, 0, 0)";
const GREEN = "rgb(0, 255, 0)";
const BLUE = "rgb(0, 0, 255)";

/** The squares' colours in the order the face draws them: the two 1s, then 2, 3 and 5. */
function square_colors_at(hour: number, minute: number): string[] {
  freeze_clock(new Date(2026, 9, 11, hour, minute, 0).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  const squares = [...container.querySelectorAll<HTMLElement>("[data-widget=column]")].filter(
    (column) => column.style.position === "absolute",
  );
  return squares.map((square) => square.style.background);
}

// ============================================================================
// Tests
// ============================================================================

describe("fibonacci clock", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
  });

  it("shows 4:50 as 3 + 1 for the hour and 5 + 3 + 2 for the minutes", () => {
    expect(square_colors_at(4, 50)).toEqual([WHITE, RED, GREEN, BLUE, GREEN]);
  });

  it("shows 23:58 as 5 + 3 + 2 + 1 for both the hour and the minutes", () => {
    expect(square_colors_at(23, 58)).toEqual([WHITE, BLUE, BLUE, BLUE, BLUE]);
  });

  it("leaves every square white at midnight", () => {
    expect(square_colors_at(0, 0)).toEqual([WHITE, WHITE, WHITE, WHITE, WHITE]);
  });
});
