import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { install_apps, open, open_app, OpenScreen, press_button, start_scheduler, swipe } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function last_event_text(container: Element): string {
  return [...container.querySelectorAll("[data-widget=label]")].at(-1)?.textContent ?? "";
}

describe("template", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("shows its name and no event at first", () => {
    const { container } = render(<OpenScreen />);
    expect(container.querySelector("[data-widget=label]")?.textContent).toBe("Template");
    expect(last_event_text(container)).toBe("");
  });

  it("names the last button press, swipe or touch", () => {
    const { container } = render(<OpenScreen />);
    act(() => press_button());
    expect(last_event_text(container)).toBe("Button");
    act(() => swipe("up"));
    expect(last_event_text(container)).toBe("Swipe up");
    act(() => swipe("down"));
    expect(last_event_text(container)).toBe("Swipe down");
    act(() => swipe("left"));
    expect(last_event_text(container)).toBe("Swipe left");
    act(() => (container.querySelector("[data-widget=screen]") as HTMLElement).click());
    expect(last_event_text(container)).toBe("(0, 0)");
  });

  it("leaves on a right swipe", () => {
    render(<OpenScreen />);
    act(() => swipe("right"));
    expect(open_app.value).toBeNull();
  });

  it("leaves on a long press of the button", () => {
    render(<OpenScreen />);
    act(() => press_button("long_press"));
    expect(open_app.value).toBeNull();
  });
});
