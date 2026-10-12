import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "neotime";
import { install_apps, open, open_app, OpenScreen, start_scheduler, swipe } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

describe("haiku", () => {
  beforeEach(() => {
    reset_persisted(app);
    install_apps([app]);
    stop_scheduler = start_scheduler();
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("starts with the first verse and moves on each time it opens", () => {
    open(app);
    render(<OpenScreen />);
    expect(screen.getByText("Morning on my wrist")).toBeTruthy();
    expect(screen.getByText("a small light wakes before me")).toBeTruthy();
    expect(screen.getByText("and counts every step")).toBeTruthy();
    act(() => open(null));
    act(() => open(app));
    expect(screen.getByText("Rain against the glass")).toBeTruthy();
  });

  it("moves on to the next verse on a tap or a swipe down, back to the first after the last", () => {
    open(app);
    render(<OpenScreen />);
    act(() => (document.querySelector("[data-widget=screen]") as HTMLElement).click());
    expect(screen.getByText("Rain against the glass")).toBeTruthy();
    act(() => swipe("down"));
    expect(screen.getByText("Seconds fall like leaves")).toBeTruthy();
    for (let verse = 0; verse < 3; verse++) act(() => swipe("down"));
    expect(screen.getByText("Swipe up to leave me")).toBeTruthy();
    act(() => swipe("down"));
    expect(screen.getByText("Morning on my wrist")).toBeTruthy();
  });

  it("ignores a left swipe and leaves on an up or right swipe", () => {
    open(app);
    render(<OpenScreen />);
    act(() => swipe("left"));
    expect(screen.getByText("Morning on my wrist")).toBeTruthy();
    act(() => swipe("up"));
    expect(open_app.value).toBeNull();
    act(() => open(app));
    act(() => swipe("right"));
    expect(open_app.value).toBeNull();
  });
});
