import { act, cleanup, fireEvent, render } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { reset_persisted } from "battletime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import beacon from "./app.tsx";

afterEach(cleanup);

/** The LED settings asked for so far, as [on, brightness, period]. */
function led_requests(): unknown[][] {
  return effect_log.value
    .filter((effect) => effect.kind === "set_heart_rate_led")
    .map((effect) => [effect.on, effect.brightness, effect.period]);
}

describe("beacon", () => {
  beforeAll(() => {
    install_apps([beacon]);
    start_scheduler();
  });

  it("drives the LED from the switch and sliders, and forgets it is on when closed", () => {
    reset_persisted(beacon);
    open(beacon);
    const { container } = render(<OpenScreen />);
    effect_log.value = [];
    act(() => { (container.querySelector("[role=switch]") as HTMLElement).click(); });
    expect(led_requests()).toEqual([[true, 0, 800]]);

    const [intensity, frequency] = container.querySelectorAll("input[type=range]");
    act(() => { fireEvent.input(intensity, { target: { value: "3" } }); });
    act(() => { fireEvent.input(frequency, { target: { value: "7" } }); });
    expect(led_requests().at(-1)).toEqual([true, 3, 0]);

    act(() => { open(null); });
    act(() => { open(beacon); });
    expect(container.querySelector("[role=switch]")?.getAttribute("aria-checked")).toBe("false");
  });
});
