import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { deliver, effect_log, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function send_step(distance: number, progress_percent: number) {
  act(() => deliver("navigation", { turn: "left", instruction: "Turn left onto Main Street", distance, progress_percent }));
}

function bar_fill(): HTMLElement {
  return document.querySelector("[data-widget=bar] > div") as HTMLElement;
}

describe("navigation", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("asks the phone for the route on open and waits for a step", () => {
    expect(effect_log.value.some((effect) => effect.kind === "request_navigation_state")).toBe(true);
    expect(screen.getByText("Navigation")).toBeTruthy();
    expect(screen.getByText("--M")).toBeTruthy();
  });

  it("shows the instruction and the distance in metres, then kilometres", () => {
    send_step(250, 10);
    expect(screen.getByText("Turn left onto Main Street")).toBeTruthy();
    expect(screen.getByText("250 m")).toBeTruthy();
    send_step(12345, 10);
    expect(screen.getByText("12.3 km")).toBeTruthy();
  });

  it("fills the bar with the route's progress and turns it red near the end", () => {
    send_step(100, 50);
    expect(bar_fill().style.width).toBe("50%");
    expect(bar_fill().style.background).toBe("rgb(255, 176, 0)");
    send_step(100, 95);
    expect(bar_fill().style.background).toBe("rgb(255, 0, 0)");
  });

  it("forgets the last step when it opens again", () => {
    send_step(100, 50);
    act(() => open(null));
    act(() => open(app));
    expect(screen.getByText("Navigation")).toBeTruthy();
  });
});
