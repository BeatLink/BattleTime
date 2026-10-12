import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { battery_millivolts, battery_percent, is_charging } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, unfreeze_clock } from "neotime/preview";
import app from "./app.tsx";

/** A moment on a whole ten seconds, so the charging bar starts at the charge. */
const START = 1_767_603_600_000;

function bar_width(container: Element): string {
  const fill = container.querySelector("[data-widget=bar] > div") as HTMLElement;
  return fill.style.width;
}

function set_battery(percent: number, millivolts: number, charging: boolean) {
  act(() => {
    battery_percent.value = percent;
    battery_millivolts.value = millivolts;
    is_charging.value = charging;
  });
}

describe("battery", () => {
  beforeEach(() => {
    install_apps([app]);
    freeze_clock(START);
  });

  afterEach(() => {
    cleanup();
    unfreeze_clock();
    open(null);
    set_battery(80, 3900, false);
  });

  it("shows the charge, that it is discharging and the voltage", () => {
    set_battery(87, 4056, false);
    open(app);
    const { container } = render(<OpenScreen />);
    expect(screen.getByText("87%")).toBeTruthy();
    expect(screen.getByText("Discharging")).toBeTruthy();
    expect(screen.getByText("4.05 volts")).toBeTruthy();
    expect(bar_width(container)).toBe("87%");
  });

  it("names a full, low and critical battery", () => {
    open(app);
    render(<OpenScreen />);
    set_battery(100, 4180, false);
    expect(screen.getByText("Fully charged")).toBeTruthy();
    set_battery(12, 3680, false);
    expect(screen.getByText("Battery low")).toBeTruthy();
    expect(screen.getByText("3.68 volts")).toBeTruthy();
    set_battery(4, 3600, false);
    expect(screen.getByText("Battery critical")).toBeTruthy();
  });

  it("warns to charge below ten percent, until the charger is on", () => {
    open(app);
    render(<OpenScreen />);
    set_battery(9, 3640, false);
    expect(screen.getByText("Charge the watch")).toBeTruthy();
    set_battery(9, 3700, true);
    expect(screen.queryByText("Charge the watch")).toBeNull();
    expect(screen.getByText("Charging")).toBeTruthy();
    expect(screen.getByText("3.70 volts")).toBeTruthy();
  });

  it("fills the bar from the charge to full a second at a time while charging", () => {
    set_battery(60, 3950, true);
    open(app);
    const { container } = render(<OpenScreen />);
    expect(bar_width(container)).toBe("60%");
    act(() => advance_clock(1000));
    expect(bar_width(container)).toBe("70%");
    act(() => advance_clock(3000));
    expect(bar_width(container)).toBe("100%");
    act(() => advance_clock(1000));
    expect(bar_width(container)).toBe("60%");
  });
});
