import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { app_memory_budget, packages } from "neotime";
import { install_apps, open, OpenScreen, screen_stack, start_scheduler } from "neotime/preview";
import app, { kilobytes, refusal_text, toggle } from "./app.tsx";

let stop_scheduler = () => {};

describe("software", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    packages.value = [
      { id: "digital", name: "Digital", is_face: true, enabled: true, memory: 12_700, refusal: 4 },
      { id: "dice", name: "Dice", is_face: false, enabled: false, memory: 11_700, refusal: 0 },
      { id: "orrery", name: "Orrery", is_face: false, enabled: false, memory: 41_000, refusal: 1 },
    ];
    app_memory_budget.value = 27_648;
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
    packages.value = [];
  });

  it("lists every package with its memory", () => {
    const { container } = render(<OpenScreen />);
    expect(container.textContent).toContain("Digital");
    expect(container.textContent).toContain("Face, 13 KB");
    expect(container.textContent).toContain("Each app may use 27 KB");
  });

  it("turns an app on when nothing refuses it", () => {
    act(() => toggle(1));
    expect(packages.value[1].enabled).toBe(true);
    expect(screen_stack.value).toEqual(["main"]);
  });

  it("says why when the watch would refuse", () => {
    const { container } = render(<OpenScreen />);
    act(() => toggle(2));
    expect(packages.value[2].enabled).toBe(false);
    expect(screen_stack.value).toEqual(["main", "refused"]);
    expect(container.textContent).toContain("Needs more memory than an app can have");
    expect(container.textContent).toContain("Needs 41 KB of 27 KB");
  });

  it("words every refusal and rounds memory up", () => {
    expect([1, 2, 3, 4, 5].map(refusal_text).every((text) => text.length > 0)).toBe(true);
    expect(refusal_text(0)).toBe("");
    expect(kilobytes(1025)).toBe("2 KB");
    expect(kilobytes(1024)).toBe("1 KB");
  });
});
