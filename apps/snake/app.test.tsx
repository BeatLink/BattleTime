import { describe, expect, it } from "vitest";
import { hits_wall } from "./app.tsx";

describe("snake rules", () => {
  it("stops at every wall", () => {
    expect(hits_wall(0, -1)).toBe(true);
    expect(hits_wall(0, -16)).toBe(true);
    expect(hits_wall(15, 1)).toBe(true);
    expect(hits_wall(16, -1)).toBe(true);
    expect(hits_wall(255, 16)).toBe(true);
    expect(hits_wall(136, 1)).toBe(false);
    expect(hits_wall(239, 16)).toBe(false);
  });
});
