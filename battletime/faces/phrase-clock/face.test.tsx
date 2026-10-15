// Checks the phrase for each of the original's rules, and the date's lower-case wording.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import { freeze_clock, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

function shown_at(hour: number, minute: number): string {
  freeze_clock(new Date(2021, 2, 6, hour, minute, 0).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return container.textContent ?? "";
}

describe("phrase-clock", () => {
  afterEach(() => {
    cleanup();
    unfreeze_clock();
  });

  it("says each kind of phrase as the original does", () => {
    const cases: [number, number, string[]][] = [
      [9, 0, ["nine", "o' clock"]],
      [9, 1, ["one minute", "past nine"]],
      [9, 15, ["quarter past", "nine"]],
      [9, 19, ["nineteen minutes", "past nine"]],
      [9, 30, ["half past", "nine"]],
      [8, 41, ["nineteen minutes", "to nine"]],
      [9, 45, ["quarter to", "ten"]],
      [9, 59, ["one minute", "to ten"]],
    ];
    for (const [hour, minute, lines] of cases) {
      const text = shown_at(hour, minute);
      for (const line of lines) expect(text).toContain(line);
      cleanup();
    }
  });

  it("counts past the hour on a 24 hour dial and to the hour on a 12 hour one", () => {
    expect(shown_at(13, 20)).toContain("past thirteen");
    cleanup();
    expect(shown_at(23, 40)).toContain("to twelve");
    cleanup();
    expect(shown_at(12, 50)).toContain("to one");
    cleanup();
    expect(shown_at(0, 0)).toContain("zero");
  });

  it("writes the date in lower case with a dotted weekday", () => {
    expect(shown_at(8, 41)).toContain("sat. 6 mar 2021");
  });
});
