import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { current_call } from "battletime";
import { deliver, effect_log, install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

function effect_kinds(): unknown[] {
  return effect_log.value.map((effect) =>
    effect.kind === "call_control" ? `call_control ${effect.command as string}` : effect.kind,
  );
}

function call(kind: "incoming" | "answered" | "ended") {
  act(() => deliver("call", { kind, caller: "Sam" }));
}

describe("call", () => {
  beforeEach(() => {
    current_call.value = { caller: "", state: "ended" };
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
    effect_log.value = [];
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("says there is no call until one comes", () => {
    expect(screen.getByText("No call")).toBeTruthy();
  });

  it("shows who is calling and answers", () => {
    call("incoming");
    expect(screen.getByText("Incoming call from")).toBeTruthy();
    expect(screen.getByText("Sam")).toBeTruthy();
    act(() => screen.getByText("Accept").click());
    expect(effect_kinds()).toEqual(["call_control answer"]);
    call("answered");
    expect(screen.getByText("On call with")).toBeTruthy();
  });

  it("mutes once, until the next call", () => {
    call("incoming");
    act(() => screen.getByText("Mute").click());
    expect(effect_kinds()).toEqual(["call_control mute"]);
    expect((screen.getByText("Muted").closest("button") as HTMLButtonElement).disabled).toBe(true);
    call("incoming");
    expect(screen.getByText("Mute")).toBeTruthy();
  });

  it("rejects and leaves", () => {
    call("incoming");
    act(() => screen.getByText("Reject").click());
    expect(effect_kinds()).toEqual(["call_control reject", "back"]);
  });

  it("ends an answered call and leaves", () => {
    call("answered");
    act(() => screen.getByText("End call").click());
    expect(effect_kinds()).toEqual(["call_control reject", "back"]);
  });

  it("leaves when the phone ends the call", () => {
    call("incoming");
    call("ended");
    expect(effect_kinds()).toEqual(["back"]);
  });
});
