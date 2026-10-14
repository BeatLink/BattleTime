import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "neotime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

/** Opens the remote with the button file the phone wrote, as bytes. */
function open_with_buttons(...bytes: number[]) {
  preview_files.value = { "/remote/buttons": String.fromCharCode(...bytes) };
  open(app);
  render(<OpenScreen />);
}

function shown_buttons(): HTMLElement[] {
  return screen.queryAllByRole("button").filter((button) => button.style.display !== "none");
}

let stop_scheduler = () => {};

describe("remote", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
    preview_files.value = {};
  });

  it("asks for buttons while the phone has set none", () => {
    open_with_buttons();

    expect(screen.getByText(/Choose buttons/)).toBeTruthy();
  });

  it("ignores a file in another format", () => {
    open_with_buttons(2, 3, 1, 2, 3);

    expect(screen.getByText(/Choose buttons/)).toBeTruthy();
  });

  it("shows the phone's buttons and tells the phone which one was tapped", () => {
    open_with_buttons(1, 3, 31, 32, 28);

    expect(screen.queryByText(/Choose buttons/)).toBeNull();
    const buttons = shown_buttons();
    expect(buttons).toHaveLength(3);
    act(() => buttons[2].click());

    expect(effect_log.value).toContainEqual({ kind: "vibrate_pattern", pattern: "tap" });
    expect(effect_log.value).toContainEqual({ kind: "send_to_phone", channel: "remote", data: { button: 2 } });
  });

  it("lays out one, two, six and nine buttons, and shows nine of a longer list", () => {
    const sizes: string[] = [];
    for (const count of [1, 2, 6, 12]) {
      open_with_buttons(1, count, ...Array.from({ length: count }, () => 31));
      const buttons = shown_buttons();
      sizes.push(`${buttons.length} ${buttons[0].style.width} ${buttons[0].style.height}`);
      cleanup();
      open(null);
    }

    expect(sizes).toEqual(["1 224px 224px", "2 108px 224px", "6 69px 108px", "9 69px 69px"]);
  });
});
