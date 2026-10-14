// Checks the time, the blinking colon, the date on both clock types and the backdrops a long press steps through.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { reset_persisted, settings } from "neotime";
import { freeze_clock, unfreeze_clock } from "neotime/preview";
import face from "./face.tsx";

function draw_at(hour: number, minute: number, second = 0): string[] {
  freeze_clock(new Date(2026, 9, 11, hour, minute, second).getTime());
  const Face = face.screens.main;
  const { container } = render(<Face />);
  return [...container.querySelectorAll("[data-widget=label]")].map((label) => label.textContent ?? "");
}

function backdrop(): string {
  return (document.querySelector("[data-widget=screen]") as HTMLElement).style.background;
}

describe("lite photo", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    unfreeze_clock();
    reset_persisted(face);
    settings.uses_24_hour_clock.value = true;
  });

  it("shows the time with a colon on even seconds and the date day first", () => {
    const labels = draw_at(8, 5, 0);
    expect(labels.slice(0, 3)).toEqual(["08", ":", "05"]);
    expect(labels).toContain("Sunday, 11 October");
    expect(labels).not.toContain("AM");
  });

  it("blanks the colon on odd seconds", () => {
    expect(draw_at(8, 5, 1)[1]).toBe(" ");
  });

  it("drops the leading zero, adds AM or PM and puts the month first on a 12 hour clock", () => {
    settings.uses_24_hour_clock.value = false;
    let labels = draw_at(20, 5);
    expect(labels[0]).toBe("8");
    expect(labels).toContain("PM");
    expect(labels).toContain("Sunday, October 11");
    cleanup();
    labels = draw_at(0, 5);
    expect(labels[0]).toBe("12");
    expect(labels).toContain("AM");
  });

  it("steps through the three backdrops on a long press", () => {
    vi.useFakeTimers();
    draw_at(8, 5);
    const seen = [backdrop()];
    for (let press = 0; press < 3; press++) {
      act(() => {
        fireEvent.pointerDown(document.querySelector("[data-widget=screen]")!);
        vi.advanceTimersByTime(500);
      });
      act(() => { fireEvent.pointerUp(document.querySelector("[data-widget=screen]")!); });
      seen.push(backdrop());
    }
    expect(new Set(seen.slice(0, 3)).size).toBe(3);
    expect(seen[3]).toBe(seen[0]);
    expect(screen.getByText("08")).toBeTruthy();
  });
});
