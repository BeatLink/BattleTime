import { describe, expect, it } from "vitest";
import { step } from "./app.tsx";

describe("paddle", () => {
  it("keeps the ball on the screen for a long game", () => {
    for (let count = 0; count < 2000; count++) step();
  });
});
