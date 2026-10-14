import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "neotime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

let stop_scheduler = () => {};

describe("prompt", () => {
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

  it("shows the phone's question and sends yes before leaving", () => {
    preview_files.value = { "/temp/prompt/text": "Leave now?" };
    open(app);
    render(<OpenScreen />);
    expect(screen.getByText("Leave now?")).toBeTruthy();

    act(() => screen.getByText("Yes").click());

    expect(effect_log.value).toContainEqual({ kind: "send_to_phone", channel: "prompt", data: { answer: 1 } });
    expect(effect_log.value).toContainEqual({ kind: "back" });
  });

  it("sends no when no is tapped", () => {
    preview_files.value = { "/temp/prompt/text": "Leave now?" };
    open(app);
    render(<OpenScreen />);

    act(() => screen.getByText("No").click());

    expect(effect_log.value).toContainEqual({ kind: "send_to_phone", channel: "prompt", data: { answer: 0 } });
  });

  it("says so when the phone sent no question", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No question from the phone/)).toBeTruthy();
  });
});
