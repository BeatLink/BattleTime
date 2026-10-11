import { cleanup, render } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { advance_clock, effect_log, freeze_clock, install_apps, open, OpenScreen, press_button, start_scheduler } from "neotime/preview";
import metronome from "./app.tsx";

afterEach(cleanup);

/** The buzz lengths asked for so far. */
function buzzes(): number[] {
  return effect_log.value.filter((effect) => effect.kind === "vibrate").map((effect) => effect.milliseconds as number);
}

function advance_in_small_steps(milliseconds: number) {
  for (let passed = 0; passed < milliseconds; passed += 100) advance_clock(100);
}

describe("metronome", () => {
  beforeAll(() => {
    install_apps([metronome]);
    start_scheduler();
  });

  it("beats on time with a long buzz opening each bar", () => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    open(metronome);
    render(<OpenScreen />);
    effect_log.value = [];
    press_button();
    advance_in_small_steps(2000);
    expect(buzzes()).toEqual([90, 30, 30, 30, 90]);
    press_button();
    advance_in_small_steps(2000);
    expect(buzzes().length).toBe(5);
  });
});
