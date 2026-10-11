import { describe, expect, it } from "vitest";
import { canvas, paint } from "./app.tsx";

describe("infinipaint", () => {
  it("paints only the cell under the finger with the brush", () => {
    const before = [...canvas.value];
    paint({ value: 22 });
    const changed = canvas.value.flatMap((colour, cell) => (colour !== before[cell] ? [cell] : []));
    expect(changed).toEqual([22]);
    expect(canvas.value[22]).toBe(2);
  });
});
