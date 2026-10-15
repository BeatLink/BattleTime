import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  accelerometer_chip_id,
  allocation_failures,
  battery_millivolts,
  battery_percent,
  bluetooth_address,
  bootloader_build_date,
  bootloader_crc,
  bootloader_version,
  current_screen,
  firmware_version,
  flash_id,
  free_heap_bytes,
  free_storage_kilobytes,
  go,
  is_charging,
  least_free_heap_bytes,
  reset_reason,
  stack_overflows,
  started_at,
  tasks,
  touch_chip_id,
  DAY,
  HOUR,
} from "battletime";
import {
  advance_clock,
  freeze_clock,
  install_apps,
  open,
  OpenScreen,
  start_scheduler,
  swipe,
  unfreeze_clock,
} from "battletime/preview";
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

/** The page bar's seven segments with the system page on show, and with the battery page. */
const PAGE_BAR_ON_SYSTEM = ["rgb(160, 160, 160)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)"];
const PAGE_BAR_ON_BATTERY = ["rgb(56, 56, 56)", "rgb(160, 160, 160)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)", "rgb(56, 56, 56)"];

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
    expect(page_bar_colors(container)).toEqual(PAGE_BAR_ON_BATTERY);
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
    expect(page_bar_colors(container)).toEqual(PAGE_BAR_ON_SYSTEM);
    act(() => swipe("left"));
    expect(page_on_show(container)).toBe("1");
    expect(page_bar_colors(container)).toEqual(PAGE_BAR_ON_BATTERY);
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

  it("shows the version, uptime and last restart, and the free storage on the memory page", () => {
    render(<OpenScreen />);
    expect(screen.getByText("0.1.0+1b19aed")).toBeTruthy();
    expect(screen.getByText(/Uptime +1d 02:00:00/)).toBeTruthy();
    expect(screen.getByText("Fault at 0x0001a2b4")).toBeTruthy();
    act(() => swipe("left"));
    act(() => swipe("left"));
    expect(screen.getByText(/Storage free +2\.7 MB/)).toBeTruthy();
  });

  it("counts the uptime on and follows the storage", () => {
    render(<OpenScreen />);
    act(() => advance_clock(5000));
    expect(screen.getByText(/1d 02:00:05/)).toBeTruthy();
    act(() => swipe("left"));
    act(() => swipe("left"));
    act(() => {
      free_storage_kilobytes.value = 300;
    });
    expect(screen.getByText(/Storage free +300 KB/)).toBeTruthy();
  });
});

describe("system: the about pages", () => {
  beforeEach(() => {
    freeze_clock(START);
    started_at.value = START - 10 * HOUR;
    install_apps([app]);
    open(app);
  });

  afterEach(() => {
    cleanup();
    unfreeze_clock();
    open(null);
    bootloader_version.value = "0.1.0+preview";
    bootloader_crc.value = 0x1a2b3c4d;
    touch_chip_id.value = 0xb4;
    accelerometer_chip_id.value = 0x13;
    flash_id.value = 0x0b4016;
    tasks.value = [];
  });

  /** Renders the app and swipes left `count` times, to the page that far from the system page. */
  function turn_to_page(count: number) {
    render(<OpenScreen />);
    for (let page = 0; page < count; page++) act(() => swipe("left"));
  }

  it("shows the heap and its counts on the memory page", () => {
    free_heap_bytes.value = 18_432;
    least_free_heap_bytes.value = 12_288;
    allocation_failures.value = 2;
    stack_overflows.value = 1;
    turn_to_page(2);
    expect(screen.getByText(/Heap free +18\.0 KB/)).toBeTruthy();
    expect(screen.getByText(/Least free +12\.0 KB/)).toBeTruthy();
    expect(screen.getByText(/Failed allocs +2/)).toBeTruthy();
    expect(screen.getByText(/Stack overflows +1/)).toBeTruthy();
  });

  it("shows the bootloader's version and build date, or the start of its CRC", () => {
    bootloader_version.value = "0.1.0+0c4e2d1";
    bootloader_build_date.value = "2025-12-20";
    turn_to_page(3);
    expect(screen.getByText(/0\.1\.0\+0c4e2d1/)).toBeTruthy();
    expect(screen.getByText(/Built 2025-12-20/)).toBeTruthy();
    act(() => {
      bootloader_version.value = "";
    });
    expect(screen.getByText("CRC 1a2b3c4")).toBeTruthy();
    act(() => {
      bootloader_crc.value = null;
    });
    expect(screen.getByText("Unreadable")).toBeTruthy();
  });

  it("writes the address and IDs in hex and names the chips it knows", () => {
    bluetooth_address.value = 0xd12a5c7e903b;
    turn_to_page(4);
    expect(screen.getByText("D1:2A:5C:7E:90:3B")).toBeTruthy();
    expect(screen.getByText(/Flash +XT25F32B 0B4016/)).toBeTruthy();
    expect(screen.getByText(/Touch +CST816S v2/)).toBeTruthy();
    expect(screen.getByText(/Accelerometer +BMA425/)).toBeTruthy();
    act(() => {
      flash_id.value = 0xc84016;
      touch_chip_id.value = 0xb5;
      accelerometer_chip_id.value = 0x11;
    });
    expect(screen.getByText(/Flash +C84016/)).toBeTruthy();
    expect(screen.getByText(/CST816T v2/)).toBeTruthy();
    expect(screen.getByText(/BMA421/)).toBeTruthy();
    act(() => {
      flash_id.value = 0;
      touch_chip_id.value = 0x20;
      accelerometer_chip_id.value = 0;
    });
    expect(screen.getByText(/Flash +No answer/)).toBeTruthy();
    expect(screen.getByText(/Chip 32 v2/)).toBeTruthy();
    expect(screen.getByText(/Accelerometer +No answer/)).toBeTruthy();
    act(() => {
      touch_chip_id.value = 0xb6;
    });
    expect(screen.getByText(/CST816D v2/)).toBeTruthy();
  });

  it("lists each task's runs, in thousands past a hundred thousand, and its share of the time", () => {
    tasks.value = [
      { name: "shell", runs: 52_100, busy_milliseconds: 360_000 },
      { name: "motion", runs: 2_400_000, busy_milliseconds: 0 },
      { name: "storage", runs: 9, busy_milliseconds: 36_000 },
    ];
    turn_to_page(5);
    expect(screen.getByText(/shell +52100 +1\.0%/)).toBeTruthy();
    expect(screen.getByText(/motion +2400k +0\.0%/)).toBeTruthy();
    expect(screen.getByText(/storage +9 +0\.1%/)).toBeTruthy();
  });

  it("ends on the licence", () => {
    turn_to_page(6);
    expect(screen.getByText(/GNU GPL, version 3 or later/)).toBeTruthy();
  });
});
