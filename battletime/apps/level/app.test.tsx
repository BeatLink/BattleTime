import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "battletime";
import { deliver, install_apps, open, OpenScreen, screen_stack, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function bubble(): HTMLElement {
  return [...document.querySelectorAll("[data-widget=stack]")].at(-1) as HTMLElement;
}

function tilt(x: number, y: number) {
  act(() => deliver("acceleration", { x, y, z: -1000 }));
}

function open_menu() {
  act(() => (document.querySelector("[data-widget=screen]") as HTMLElement).click());
}

describe("level", () => {
  beforeEach(() => {
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("sits green in the centre while the watch lies flat", () => {
    tilt(6, -6);
    expect(bubble().style.left).toBe("111px");
    expect(bubble().style.top).toBe("115px");
    expect(bubble().style.background).toBe("rgb(32, 160, 64)");
  });

  it("moves the bubble against the tilt and keeps it inside the card", () => {
    tilt(300, -150);
    expect(bubble().style.left).toBe("13px");
    expect(bubble().style.top).toBe("163px");
    expect(bubble().style.background).toBe("rgb(255, 255, 255)");
    tilt(-900, 900);
    expect(bubble().style.left).toBe("213px");
    expect(bubble().style.top).toBe("13px");
  });

  it("calibrates on the current reading and resets from the menu", () => {
    tilt(300, 300);
    open_menu();
    expect(screen_stack.value.at(-1)).toBe("menu");
    act(() => screen.getByText("Calibrate").click());
    expect(screen_stack.value).toEqual(["main"]);
    expect(bubble().style.left).toBe("113px");
    open_menu();
    act(() => screen.getByText("Reset").click());
    expect(bubble().style.left).toBe("13px");
  });

  it("leaves the calibration alone on Cancel", () => {
    tilt(300, 0);
    open_menu();
    act(() => screen.getByText("Cancel").click());
    expect(screen_stack.value).toEqual(["main"]);
    expect(bubble().style.left).toBe("13px");
  });
});
