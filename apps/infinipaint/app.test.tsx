import { describe, expect, it } from "vitest";
import { cell_at } from "./app.tsx";

describe("infinipaint", () => {
  it("finds the cell under the finger, even at the edges", () => {
    expect(cell_at(0, 0)).toBe(0);
    expect(cell_at(13, 0)).toBe(1);
    expect(cell_at(0, 13)).toBe(20);
    expect(cell_at(239, 239)).toBe(399);
    expect(cell_at(260, -5)).toBe(19);
  });
});
