import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "battletime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler, swipe } from "battletime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function last_brightness(): unknown {
  return effect_log.value.filter((effect) => effect.kind === "set_brightness").at(-1)?.level;
}

function screen_of(container: Element): HTMLElement {
  return container.querySelector("[data-widget=screen]") as HTMLElement;
}

/** How many of the three level marks are filled in. */
function filled_marks(container: Element): number {
  const marks = [...container.querySelectorAll("[data-widget=row] > [data-widget=column]")] as HTMLElement[];
  return marks.filter((mark) => mark.style.background === "rgb(96, 96, 96)").length;
}

describe("flashlight", () => {
  beforeEach(() => {
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("opens lit white at full brightness and keeps the screen on", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    expect(screen_of(container).style.background).toBe("rgb(255, 255, 255)");
    expect(last_brightness()).toBe(3);
    expect(effect_log.value.some((effect) => effect.kind === "keep_screen_on" && effect.on === true)).toBe(true);
    expect(filled_marks(container)).toBe(3);
  });

  it("goes dark at the lowest brightness on a tap and lights again on the next", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    act(() => screen_of(container).click());
    expect(screen_of(container).style.background).toBe("rgb(0, 0, 0)");
    expect(last_brightness()).toBe(1);
    act(() => screen_of(container).click());
    expect(screen_of(container).style.background).toBe("rgb(255, 255, 255)");
    expect(last_brightness()).toBe(3);
  });

  it("steps the brightness with left and right swipes, within one to three", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    act(() => swipe("left"));
    expect(last_brightness()).toBe(2);
    expect(filled_marks(container)).toBe(2);
    act(() => swipe("left"));
    act(() => swipe("left"));
    expect(last_brightness()).toBe(1);
    expect(filled_marks(container)).toBe(1);
    act(() => swipe("right"));
    act(() => swipe("right"));
    act(() => swipe("right"));
    expect(last_brightness()).toBe(3);
  });

  it("switches between white and red light on an up or down swipe", () => {
    open(app);
    const { container } = render(<OpenScreen />);
    act(() => swipe("up"));
    expect(screen_of(container).style.background).toBe("rgb(255, 0, 0)");
    act(() => swipe("down"));
    expect(screen_of(container).style.background).toBe("rgb(255, 255, 255)");
  });
});
