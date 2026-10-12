import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { is_phone_connected } from "neotime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler, toast } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function find_phone_requests(): unknown[] {
  return effect_log.value.filter((effect) => effect.kind === "find_phone").map((effect) => effect.on);
}

describe("phone finder", () => {
  beforeEach(() => {
    is_phone_connected.value = true;
    install_apps([app]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("rings the phone on a tap and stops it on the next", () => {
    render(<OpenScreen />);
    act(() => screen.getByText("Ring").click());
    expect(find_phone_requests()).toEqual([true]);
    expect(toast.value).toBe("Ringing your phone");
    act(() => screen.getByText("Stop").click());
    expect(find_phone_requests()).toEqual([true, false]);
    expect(screen.getByText("Ring")).toBeTruthy();
  });

  it("stops the ringing when it closes", () => {
    render(<OpenScreen />);
    act(() => screen.getByText("Ring").click());
    open(null);
    expect(find_phone_requests()).toEqual([true, false]);
  });

  it("asks nothing of the phone when closed without ringing", () => {
    open(null);
    expect(find_phone_requests()).toEqual([]);
  });

  it("says so and greys out the button while no phone is connected", () => {
    is_phone_connected.value = false;
    render(<OpenScreen />);
    expect(screen.getByText("Phone not connected")).toBeTruthy();
    expect((screen.getByRole("button") as HTMLButtonElement).disabled).toBe(true);
  });
});
