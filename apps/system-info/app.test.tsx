import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { battery_percent, firmware_version, free_storage_kilobytes, is_charging, reset_reason, started_at, DAY, HOUR } from "neotime";
import { advance_clock, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

const START = new Date(2026, 0, 5, 9, 0, 0).getTime();

let stop_scheduler = () => {};

describe("system info", () => {
  beforeEach(() => {
    freeze_clock(START);
    firmware_version.value = "0.1.0+1b19aed";
    started_at.value = START - DAY - 2 * HOUR;
    battery_percent.value = 87;
    is_charging.value = false;
    free_storage_kilobytes.value = 2840;
    reset_reason.value = "Fault at 0x0001a2b4";
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("shows the version, uptime, last restart, battery and free storage", () => {
    render(<OpenScreen />);
    expect(screen.getByText("0.1.0+1b19aed")).toBeTruthy();
    expect(screen.getByText("1d 02:00:00")).toBeTruthy();
    expect(screen.getByText("Fault at 0x0001a2b4")).toBeTruthy();
    expect(screen.getByText("87%")).toBeTruthy();
    expect(screen.getByText("2.7 MB free")).toBeTruthy();
  });

  it("counts the uptime on and follows the battery and storage", () => {
    render(<OpenScreen />);
    act(() => { advance_clock(5000); });
    expect(screen.getByText("1d 02:00:05")).toBeTruthy();
    act(() => {
      is_charging.value = true;
      free_storage_kilobytes.value = 300;
    });
    expect(screen.getByText("87% charging")).toBeTruthy();
    expect(screen.getByText("300 KB free")).toBeTruthy();
  });
});
