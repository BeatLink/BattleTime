import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  battery_millivolts,
  battery_percent,
  current_screen,
  firmware_version,
  free_storage_kilobytes,
  go,
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

/** A moment on a whole ten seconds, so the charging battery starts at the charge. */
const START = 1_767_603_600_000;

/** The battery's fill height in pixels: the innermost of the meter's nested columns. */
function meter_fill_height(container: Element): number {
  const fill = container.querySelector("[data-widget=column] > [data-widget=column] > [data-widget=column]:only-child");
  return parseInt((fill as HTMLElement).style.height, 10);
}

/** Opens the app and swipes left from the system page to the battery. */
function open_battery_page(): Element {
  open(app);
  const { container } = render(<OpenScreen />);
  act(() => swipe("left"));
  return container;
}

/** The page bar's segment colours from the left, the page on show in the muted grey. */
function page_bar_colors(container: Element): string[] {
  const segments = container.querySelectorAll("[data-widget=page-bar-part]");
  return Array.from(segments).map((segment) => (segment as HTMLElement).style.background);
}

/** Which page of the app's pages is on show. */
function page_on_show(container: Element): string | null {
  return container.querySelector("[data-widget=pages]")?.getAttribute("data-page") ?? null;
}

function set_battery(percent: number, millivolts: number, charging: boolean) {
  act(() => {
    battery_percent.value = percent;
    battery_millivolts.value = millivolts;
    is_charging.value = charging;
  });
}

describe("system: the battery page", () => {
  let stop_scheduler = () => {};

  beforeEach(() => {
    install_apps([app]);
    freeze_clock(START);
    stop_scheduler = start_scheduler();
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    unfreeze_clock();
    open(null);
    set_battery(80, 3900, false);
  });

  it("shows the charge, that it is discharging and the voltage", () => {
    set_battery(87, 4056, false);
    const container = open_battery_page();
    expect(page_on_show(container)).toBe("1");
    expect(screen.getByText("87%")).toBeTruthy();
    expect(screen.getByText("Discharging")).toBeTruthy();
    expect(screen.getByText("4.05 volts")).toBeTruthy();
    expect(page_bar_colors(container)).toEqual(["rgb(56, 56, 56)", "rgb(160, 160, 160)"]);
  });

  it("names a full, low and critical battery", () => {
    open_battery_page();
    set_battery(100, 4180, false);
    expect(screen.getByText("Fully charged")).toBeTruthy();
    set_battery(12, 3680, false);
    expect(screen.getByText("Battery low")).toBeTruthy();
    expect(screen.getByText("3.68 volts")).toBeTruthy();
    set_battery(4, 3600, false);
    expect(screen.getByText("Almost empty")).toBeTruthy();
  });

  it("warns to charge below ten percent, until the charger is on", () => {
    open_battery_page();
    set_battery(9, 3640, false);
    expect(screen.getByText("Charge now")).toBeTruthy();
    expect(screen.getByText("3.64 volts")).toBeTruthy();
    set_battery(9, 3700, true);
    expect(screen.queryByText("Charge now")).toBeNull();
    expect(screen.getByText("Charging")).toBeTruthy();
    expect(screen.getByText("3.70 volts")).toBeTruthy();
  });

  it("fills the battery to the charge, and from there to full a second at a time while charging", () => {
    set_battery(50, 3800, false);
    const container = open_battery_page();
    expect(meter_fill_height(container)).toBe(80);
    set_battery(60, 3950, true);
    expect(meter_fill_height(container)).toBe(96);
    act(() => advance_clock(1000));
    expect(meter_fill_height(container)).toBe(112);
    act(() => advance_clock(3000));
    expect(meter_fill_height(container)).toBe(160);
    act(() => advance_clock(1000));
    expect(meter_fill_height(container)).toBe(96);
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

  it("is the first page, with the battery a swipe left and a swipe right back", () => {
    const { container } = render(<OpenScreen />);
    expect(current_screen.value).toBe("main");
    expect(page_on_show(container)).toBe("0");
    expect(page_bar_colors(container)).toEqual(["rgb(160, 160, 160)", "rgb(56, 56, 56)"]);
    act(() => swipe("left"));
    expect(page_on_show(container)).toBe("1");
    expect(page_bar_colors(container)).toEqual(["rgb(56, 56, 56)", "rgb(160, 160, 160)"]);
    act(() => swipe("right"));
    expect(page_on_show(container)).toBe("0");
    expect(current_screen.value).toBe("main");
  });

  it("opens on the battery page for the shell's battery screen", () => {
    act(() => go("battery"));
    const { container } = render(<OpenScreen />);
    expect(page_on_show(container)).toBe("1");
    expect(screen.getByText("Discharging")).toBeTruthy();
  });

  it("shows the version, uptime, last restart and free storage", () => {
    render(<OpenScreen />);
    expect(screen.getByText("0.1.0+1b19aed")).toBeTruthy();
    expect(screen.getByText("1d 02:00:00")).toBeTruthy();
    expect(screen.getByText("Fault at 0x0001a2b4")).toBeTruthy();
    expect(screen.getByText("2.7 MB free")).toBeTruthy();
  });

  it("counts the uptime on and follows the storage", () => {
    render(<OpenScreen />);
    act(() => advance_clock(5000));
    expect(screen.getByText("1d 02:00:05")).toBeTruthy();
    act(() => {
      free_storage_kilobytes.value = 300;
    });
    expect(screen.getByText("300 KB free")).toBeTruthy();
  });
});
