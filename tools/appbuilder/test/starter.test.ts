import { describe, expect, it } from "vitest";

import { is_app_id, name_from_id, starter_app } from "../src/starter";

describe("a new app", () => {
  it("takes ids written like the folders in apps/", () => {
    expect(is_app_id("four-in-a-row")).toBe(true);
    expect(is_app_id("timer2")).toBe(true);
    for (const bad of ["", "Timer", "two words", "-x", "x-", "a--b", "2048", "a/b"]) expect(is_app_id(bad)).toBe(false);
  });

  it("gets its name and screen from its id", () => {
    expect(name_from_id("four-in-a-row")).toBe("Four in a row");
    const source = starter_app("four-in-a-row");
    expect(source).toContain('id: "four-in-a-row"');
    expect(source).toContain('name: "Four in a row"');
    expect(source).toContain("function FourInARow()");
    expect(source).toContain("screens: { main: FourInARow }");
  });
});
