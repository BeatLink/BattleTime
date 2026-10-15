import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  battery_percent,
  brightness,
  define_app,
  is_bluetooth_enabled,
  is_do_not_disturb,
  set_do_not_disturb,
  settings,
} from "battletime";
import { install_apps, open, open_app, OpenScreen, start_scheduler } from "battletime/preview";
import { icons } from "battletime/icons";
import { Screen } from "battletime/widgets";
import quick_settings from "./app.tsx";

let stop_scheduler = () => {};

const flashlight = define_app({ id: "flashlight", name: "Flashlight", icon: icons.flashlight, screens: { main: () => <Screen /> } });
const settings_app = define_app({ id: "settings", name: "Settings", icon: icons.settings, screens: { main: () => <Screen /> } });

function tap(name: string) {
  act(() => (screen.getByText(name).closest("[data-widget=button]") as HTMLElement).click());
}

describe("quick settings", () => {
  beforeEach(() => {
    install_apps([quick_settings, flashlight, settings_app]);
    stop_scheduler = start_scheduler();
    open(quick_settings);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("shows the battery over its six buttons", () => {
    battery_percent.value = 64;
    render(<OpenScreen />);
    expect(screen.getByText("64%")).toBeTruthy();
    for (const name of ["Light", "Silent", "Bluetooth", "Always on", "Settings"]) {
      expect(screen.getByText(name)).toBeTruthy();
    }
  });

  it("steps the brightness round its three levels", () => {
    render(<OpenScreen />);
    const start = brightness.value;
    const names = ["Low", "Medium", "High"];
    tap(names[start - 1]);
    expect(brightness.value).toBe((start % 3) + 1);
    tap(names[brightness.value - 1]);
    tap(names[brightness.value - 1]);
    expect(brightness.value).toBe(start);
  });

  it("switches Bluetooth and the always-on screen", () => {
    render(<OpenScreen />);
    const had_bluetooth = is_bluetooth_enabled.value;
    const was_always_on = settings.always_on_screen.value;
    tap("Bluetooth");
    tap("Always on");
    expect(is_bluetooth_enabled.value).toBe(!had_bluetooth);
    expect(settings.always_on_screen.value).toBe(!was_always_on);
    tap("Bluetooth");
    tap("Always on");
    expect(is_bluetooth_enabled.value).toBe(had_bluetooth);
    expect(settings.always_on_screen.value).toBe(was_always_on);
  });

  it("steps do not disturb from off to silent to sleep and back to off", () => {
    set_do_not_disturb(false);
    render(<OpenScreen />);
    tap("Silent");
    expect(is_do_not_disturb.value).toBe(true);
    expect(settings.sleep_mode.value).toBe(false);
    tap("Silent");
    expect(is_do_not_disturb.value).toBe(true);
    expect(settings.sleep_mode.value).toBe(true);
    tap("Sleep");
    expect(is_do_not_disturb.value).toBe(false);
    expect(settings.sleep_mode.value).toBe(false);
  });

  it("opens the flashlight and Settings", () => {
    render(<OpenScreen />);
    tap("Light");
    expect(open_app.value).toBe(flashlight);
    act(() => open(quick_settings));
    tap("Settings");
    expect(open_app.value).toBe(settings_app);
  });
});
