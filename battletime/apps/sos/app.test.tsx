import { act, cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { is_phone_connected, reset_persisted } from "battletime";
import { swipe } from "battletime/widgets";
import {
  advance_clock,
  effect_log,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  send_message_to_app,
  start_scheduler,
} from "battletime/preview";
import app, {
  is_active,
  reached_phone,
  REPEAT_GAP_STEP,
  SOS_BUZZ_UNITS,
  SOS_LENGTH,
  SOS_LIT,
  use_light,
} from "./app.tsx";

let stop_scheduler = () => {};

// ============================================================================
// Helpers
// ============================================================================

/** What the app sent the phone so far, as [channel, data]. */
function phone_messages(): unknown[][] {
  return effect_log.value
    .filter((effect) => effect.kind === "send_to_phone")
    .map((effect) => [effect.channel, effect.data]);
}

function buzzes(): number[] {
  return effect_log.value.filter((effect) => effect.kind === "vibrate").map((effect) => effect.milliseconds as number);
}

function led_requests(): unknown[] {
  return effect_log.value.filter((effect) => effect.kind === "set_heart_rate_led").map((effect) => effect.on);
}

function sos_button(): HTMLElement {
  return screen.getByText("SOS").closest("[data-widget=button]") as HTMLElement;
}

function advance_in_units(milliseconds: number) {
  for (let passed = 0; passed < milliseconds; passed += 200) act(() => advance_clock(200));
}

function hold(milliseconds: number) {
  const button = sos_button();
  act(() => { fireEvent.pointerDown(button); });
  advance_in_units(milliseconds);
  return button;
}

function release(button: HTMLElement) {
  act(() => { fireEvent.pointerUp(button); });
}

// ============================================================================
// Tests
// ============================================================================

describe("sos", () => {
  beforeEach(() => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    reset_persisted(app);
    is_phone_connected.value = true;
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
    render(<OpenScreen />);
    effect_log.value = [];
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("counts down and buzzes while held, and a release before five seconds cancels without sending", () => {
    const button = hold(2000);
    expect(screen.getByText("Keep holding")).toBeTruthy();
    expect(screen.getByText("3")).toBeTruthy();
    expect(buzzes().length).toBeGreaterThanOrEqual(10);
    expect(buzzes().every((length) => length === 60)).toBe(true);
    release(button);
    effect_log.value = [];
    advance_in_units(6000);
    expect(buzzes()).toEqual([]);
    expect(phone_messages()).toEqual([]);
    expect(screen.getByText("Hold for 5 seconds")).toBeTruthy();
  });

  it("sends on 1 with one long buzz after a five second hold, and shows Stop", () => {
    hold(5000);
    expect(phone_messages()).toEqual([["sos", { on: 1 }]]);
    expect(buzzes()).toContain(1000);
    expect(screen.getByText("SOS sent")).toBeTruthy();
    expect(screen.getByText("Stop")).toBeTruthy();
  });

  it("says the phone is not connected when it signals only", () => {
    is_phone_connected.value = false;
    hold(5000);
    expect(phone_messages()).toEqual([]);
    expect(screen.getByText("Phone not connected")).toBeTruthy();
    expect(screen.getByText("Signalling only")).toBeTruthy();
    act(() => { is_phone_connected.value = true; });
  });

  it("asks before stopping, then sends on 0 and stops the signals", () => {
    hold(5000);
    effect_log.value = [];
    act(() => screen.getByText("Stop").click());
    expect(screen.getByText("Stop the SOS?")).toBeTruthy();
    act(() => screen.getByText("No").click());
    expect(phone_messages()).toEqual([]);
    expect(screen.getByText("SOS sent")).toBeTruthy();

    act(() => screen.getByText("Stop").click());
    act(() => screen.getByText("Yes").click());
    expect(phone_messages()).toEqual([["sos", { on: 0 }]]);
    expect(led_requests().at(-1)).toBe(false);
    expect(screen.getByText("SOS")).toBeTruthy();
    effect_log.value = [];
    advance_in_units(2000);
    expect(buzzes()).toEqual([]);
  });

  it("stops when the phone sends on 0, without sending on 0 back, and ignores on 1", () => {
    hold(5000);
    effect_log.value = [];
    act(() => send_message_to_app("sos", { on: 1 }));
    expect(screen.getByText("SOS sent")).toBeTruthy();

    act(() => send_message_to_app("sos", { on: 0 }));
    expect(phone_messages()).toEqual([]);
    expect(led_requests().at(-1)).toBe(false);
    expect(screen.getByText("SOS")).toBeTruthy();
    expect(is_active.value).toBe(false);
    expect(reached_phone.value).toBe(false);
  });

  it("stops while closed when the phone sends on 0, and opens again at the hold button", () => {
    hold(5000);
    open(null);
    effect_log.value = [];
    send_message_to_app("sos", { on: 0 });
    expect(is_active.value).toBe(false);
    expect(phone_messages()).toEqual([]);
    act(() => open(app));
    expect(screen.getByText("Hold for 5 seconds")).toBeTruthy();
  });

  it("sends nothing to the phone when closed, and still shows Stop when opened again", () => {
    hold(5000);
    effect_log.value = [];
    open(null);
    expect(phone_messages()).toEqual([]);
    open(app);
    expect(screen.getByText("Stop")).toBeTruthy();
  });

  it("runs the signals alone, stepping the pattern round twice, without contacting the phone", () => {
    act(() => swipe("left"));
    expect(screen.getByText("Light")).toBeTruthy();
    act(() => screen.getByText("Signal").click());
    const one_round = [200, 200, 200, 600, 600, 600, 200, 200, 200];
    // The preview keeps only the latest 50 effects, so each round is checked on its own.
    advance_in_units((SOS_LENGTH + 1) * 200);
    expect(buzzes()).toEqual(one_round);
    expect(led_requests()).toContain(true);
    effect_log.value = [];
    advance_in_units(SOS_LENGTH * 200);
    expect(buzzes()).toEqual(one_round);
    expect(phone_messages()).toEqual([]);
    act(() => screen.getByText("Stop").click());
    expect(led_requests().at(-1)).toBe(false);
    effect_log.value = [];
    advance_in_units(2000);
    expect(buzzes()).toEqual([]);
  });

  it("leaves out the light when its switch is off", () => {
    use_light.value = false;
    act(() => swipe("left"));
    act(() => screen.getByText("Signal").click());
    advance_in_units(2000);
    expect(led_requests().every((on) => on === false)).toBe(true);
  });
});

describe("the sos pattern", () => {
  it("is three dots, three dashes and three dots with the Morse gaps", () => {
    expect(SOS_LIT.length).toBe(SOS_LENGTH);
    expect(SOS_BUZZ_UNITS.length).toBe(SOS_LENGTH);
    expect(SOS_LIT.join("")).toBe("1010100011101110111000101010000000");
    expect(SOS_LIT.slice(REPEAT_GAP_STEP).every((lit) => lit === 0)).toBe(true);
  });

  it("buzzes at the start of each dot and dash for its length", () => {
    const starts = SOS_LIT.map((lit, place) => (lit === 1 && SOS_LIT[place - 1] !== 1 ? 1 : 0));
    expect(SOS_BUZZ_UNITS.map((units) => (units > 0 ? 1 : 0))).toEqual(starts);
  });

});
