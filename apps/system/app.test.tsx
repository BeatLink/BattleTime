import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  battery_millivolts,
  battery_percent,
  current_screen,
  firmware_version,
  free_storage_kilobytes,
  is_charging,
  reset_reason,
  started_at,
  DAY,
  HOUR,
} from "neotime";
import {
  advance_clock,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  start_scheduler,
  swipe,
  unfreeze_clock,
} from "neotime/preview";
import app from "./app.tsx";

/** A moment on a whole ten seconds, so the charging bar starts at the charge. */
const START = 1_767_603_600_000;

/** The ring's fill and its track, as SVG paths; a full ring's fill covers the whole track. */
function ring_paths(container: Element): { fill: string; track: string } {
  const paths = container.querySelectorAll("[data-widget=arc] path");
  return { track: paths[0].getAttribute("d") ?? "", fill: paths[1]?.getAttribute("d") ?? "" };
}

/** The scroll bar's segment colours from the top, the page on show in the muted grey. */
function scroll_bar_colors(container: Element): string[] {
  const segments = container.querySelectorAll("[data-widget=stack] > [data-widget=column]");
  return Array.from(segments).map((segment) => (segment as HTMLElement).style.background);
}

function set_battery(percent: number, millivolts: number, charging: boolean) {
  act(() => {
    battery_percent.value = percent;
    battery_millivolts.value = millivolts;
    is_charging.value = charging;
  });
}

describe("system: the battery page", () => {
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
    expect(scroll_bar_colors(container)).toEqual(["rgb(160, 160, 160)", "rgb(56, 56, 56)"]);
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

  it("fills the ring from the charge to full a second at a time while charging", () => {
    set_battery(60, 3950, true);
    open(app);
    const { container } = render(<OpenScreen />);
    const at_the_charge = ring_paths(container).fill;
    expect(at_the_charge).not.toBe(ring_paths(container).track);
    act(() => advance_clock(1000));
    expect(ring_paths(container).fill).not.toBe(at_the_charge);
    act(() => advance_clock(3000));
    expect(ring_paths(container).fill).toBe(ring_paths(container).track);
    act(() => advance_clock(1000));
    expect(ring_paths(container).fill).toBe(at_the_charge);
  });
});

describe("system: the system page", () => {
  let stop_scheduler = () => {};

  beforeEach(() => {
    freeze_clock(START);
    firmware_version.value = "0.1.0+1b19aed";
    started_at.value = START - DAY - 2 * HOUR;
    free_storage_kilobytes.value = 2840;
    reset_reason.value = "Fault at 0x0001a2b4";
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    unfreeze_clock();
    open(null);
  });

  it("is a swipe up from the battery and a swipe down back to it", () => {
    const { container } = render(<OpenScreen />);
    act(() => swipe("up"));
    expect(current_screen.value).toBe("system");
    expect(scroll_bar_colors(container)).toEqual(["rgb(56, 56, 56)", "rgb(160, 160, 160)"]);
    act(() => swipe("down"));
    expect(current_screen.value).toBe("main");
  });

  it("shows the version, uptime, last restart and free storage", () => {
    render(<OpenScreen />);
    act(() => swipe("up"));
    expect(screen.getByText("0.1.0+1b19aed")).toBeTruthy();
    expect(screen.getByText("1d 02:00:00")).toBeTruthy();
    expect(screen.getByText("Fault at 0x0001a2b4")).toBeTruthy();
    expect(screen.getByText("2.7 MB free")).toBeTruthy();
  });

  it("counts the uptime on and follows the storage", () => {
    render(<OpenScreen />);
    act(() => swipe("up"));
    act(() => advance_clock(5000));
    expect(screen.getByText("1d 02:00:05")).toBeTruthy();
    act(() => {
      free_storage_kilobytes.value = 300;
    });
    expect(screen.getByText("300 KB free")).toBeTruthy();
  });
});
