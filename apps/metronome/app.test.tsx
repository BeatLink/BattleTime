import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "neotime";
import { advance_clock, effect_log, freeze_clock, install_apps, open, OpenScreen, press_button, start_scheduler } from "neotime/preview";
import metronome from "./app.tsx";

let stop_scheduler = () => {};

/** The buzz lengths asked for so far. */
function buzzes(): number[] {
  return effect_log.value.filter((effect) => effect.kind === "vibrate").map((effect) => effect.milliseconds as number);
}

function advance_in_small_steps(milliseconds: number) {
  for (let passed = 0; passed < milliseconds; passed += 100) act(() => advance_clock(100));
}

function tap_number() {
  act(() => { fireEvent.pointerDown(document.querySelector("[data-widget=click-area]")!); });
}

describe("metronome", () => {
  beforeEach(() => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    reset_persisted(metronome);
    install_apps([metronome]);
    stop_scheduler = start_scheduler();
    open(metronome);
    render(<OpenScreen />);
    effect_log.value = [];
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("beats on time with a long buzz opening each bar", () => {
    press_button();
    advance_in_small_steps(2000);
    expect(buzzes()).toEqual([90, 30, 30, 30, 90]);
    press_button();
    advance_in_small_steps(2000);
    expect(buzzes().length).toBe(5);
  });

  it("sets the tempo from two taps on the number, within 40 to 220", () => {
    expect(screen.getByText("120")).toBeTruthy();
    tap_number();
    act(() => advance_clock(400));
    tap_number();
    expect(screen.getByText("150")).toBeTruthy();
    act(() => advance_clock(200));
    tap_number();
    expect(screen.getByText("220")).toBeTruthy();
  });

  it("starts a new tempo instead of setting one when taps are far apart", () => {
    tap_number();
    act(() => advance_clock(5000));
    tap_number();
    expect(screen.getByText("120")).toBeTruthy();
  });

  it("sets the tempo by dragging the ring", () => {
    const ring = document.querySelector("[data-widget=arc]")!;
    act(() => {
      fireEvent.pointerDown(ring);
      fireEvent.pointerMove(ring);
    });
    expect(screen.getByText("100")).toBeTruthy();
  });

  it("cycles the bar length from one to nine beats", () => {
    act(() => screen.getByText("4 bpb").click());
    expect(screen.getByText("5 bpb")).toBeTruthy();
    for (let press = 0; press < 5; press++) act(() => screen.getByRole("button", { name: /bpb/ }).click());
    expect(screen.getByText("1 bpb")).toBeTruthy();
  });

  it("carries a new tempo on from the latest beat without a jump", () => {
    act(() => screen.getAllByRole("button")[1].click());
    tap_number();
    advance_in_small_steps(1000);
    tap_number();
    expect(screen.getByText("060")).toBeTruthy();
    expect(buzzes()).toHaveLength(3);
    advance_in_small_steps(900);
    expect(buzzes()).toHaveLength(3);
    advance_in_small_steps(100);
    expect(buzzes()).toHaveLength(4);
  });

  it("stops when it closes", () => {
    press_button();
    open(null);
    expect(effect_log.value.at(-1)).toEqual({ kind: "keep_screen_on", on: false });
    open(metronome);
    advance_in_small_steps(2000);
    expect(buzzes()).toHaveLength(1);
  });
});
