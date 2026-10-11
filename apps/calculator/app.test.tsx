import { beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "neotime";
import app, { entry_text, press_key, result_text } from "./app.tsx";

const KEY_PLACES: Record<string, number> = {
  "7": 0, "8": 1, "9": 2, back: 3, clear: 4,
  "4": 5, "5": 6, "6": 7, "+": 8, "-": 9,
  "1": 10, "2": 11, "3": 12, "*": 13, "/": 14,
  "0": 15, ".": 16, sign: 17, "=": 18,
};

function press(...keys: string[]) {
  for (const key of keys) press_key({ value: KEY_PLACES[key] });
}

describe("calculator", () => {
  beforeEach(() => reset_persisted(app));

  it("adds decimals and keeps the operation for a repeat", () => {
    press("1", ".", "5", "+", "2", ".", "2", "5", "=");
    expect(result_text.value).toBe("3.75 +");
  });

  it("multiplies and divides in millionths", () => {
    press("1", "2", ".", "5", "*", "4", "=");
    expect(result_text.value).toBe("50 ×");
    press("/", "3", "=");
    expect(result_text.value).toBe("16.666666 ÷");
  });

  it("holds nine whole digits and six decimals", () => {
    press("1", "2", "3", "4", "5", "6", "7", "8", "9", "9", ".", "0", "0", "0", "0", "0", "1", "2");
    expect(entry_text.value).toBe("123456789.000001");
    press("+", "1", "=");
    expect(result_text.value).toBe("123456790.000001 +");
  });

  it("repeats the last operation on a second equals", () => {
    press("2", "+", "3", "=", "=");
    expect(result_text.value).toBe("8 +");
  });

  it("reports division by zero and too large a product", () => {
    press("5", "/", "0", "=");
    expect(entry_text.value).toBe("zero division");
    press("9", "9", "9", "9", "9", "*", "9", "9", "9", "9", "9", "=");
    expect(entry_text.value).toBe("too large");
  });

  it("keeps a negative entry below one and deletes digit by digit", () => {
    press("0", ".", "0", "5", "sign");
    expect(entry_text.value).toBe("-0.05");
    press("back", "back", "back");
    expect(entry_text.value).toBe("-0");
  });
});
