import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reset_persisted } from "neotime";
import { effect_log, install_apps, open, open_app, OpenScreen, start_scheduler, swipe, toast } from "neotime/preview";
import app, { canvas, paint } from "./app.tsx";

let stop_scheduler = () => {};

function grid(): Element {
  return document.querySelector("[data-widget=cell-grid]")!;
}

/** A finger moving over the grid, held down or not; jsdom has no pointer events, so Preact listens under this name. */
function move_over(clientX: number, clientY: number, buttons: number) {
  act(() => { grid().dispatchEvent(new MouseEvent("PointerMove", { clientX, clientY, buttons, bubbles: true })); });
}

function screen_element(): HTMLElement {
  return document.querySelector("[data-widget=screen]") as HTMLElement;
}

describe("infinipaint", () => {
  beforeEach(() => {
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    stop_scheduler();
    open(null);
  });

  it("paints only the cell under the finger with the brush", () => {
    const before = [...canvas.value];
    paint({ value: 22 });
    const changed = canvas.value.flatMap((colour, cell) => (colour !== before[cell] ? [cell] : []));
    expect(changed).toEqual([22]);
    expect(canvas.value[22]).toBe(2);
  });

  it("paints the tapped cell and every cell a drag passes over", () => {
    render(<OpenScreen />);
    act(() => { fireEvent.click(grid(), { clientX: 30, clientY: 15 }); });
    expect(canvas.value[22]).toBe(2);
    expect(grid().querySelectorAll("rect")[22].getAttribute("fill")).toBe("#ffffff");
    move_over(5, 230, 1);
    expect(canvas.value[380]).toBe(2);
    move_over(30, 230, 0);
    expect(canvas.value[382]).toBe(7);
  });

  it("changes the brush colour on a long press with a buzz and its name", () => {
    vi.useFakeTimers();
    render(<OpenScreen />);
    act(() => {
      fireEvent.pointerDown(screen_element());
      vi.advanceTimersByTime(500);
    });
    expect(toast.value).toBe("Red");
    expect(effect_log.value.some((effect) => effect.kind === "vibrate" && effect.milliseconds === 35)).toBe(true);
    paint({ value: 0 });
    expect(canvas.value[0]).toBe(3);
  });

  it("asks before clearing on a double tap, and paints nothing while asking", () => {
    paint({ value: 5 });
    render(<OpenScreen />);
    act(() => { fireEvent.dblClick(screen_element()); });
    expect(screen.getByText("Clear the picture?")).toBeTruthy();
    paint({ value: 6 });
    expect(canvas.value[6]).toBe(7);
    act(() => screen.getByText("No").click());
    expect(canvas.value[5]).toBe(2);
    act(() => { fireEvent.dblClick(screen_element()); });
    act(() => screen.getByText("Yes").click());
    expect(canvas.value.every((colour) => colour === 7)).toBe(true);
    expect(screen.queryByText("Clear the picture?")).toBeNull();
  });

  it("stays open on a swipe so a stroke never leaves the app", () => {
    render(<OpenScreen />);
    act(() => swipe("right"));
    expect(open_app.value).toBe(app);
  });
});
