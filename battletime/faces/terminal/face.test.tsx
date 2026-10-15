// Checks each tagged line's value as the watch's state changes.

import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  battery_percent,
  current_weather,
  heart_rate,
  is_bluetooth_enabled,
  is_charging,
  is_heart_rate_running,
  is_phone_connected,
  settings,
  unread_notifications,
} from "battletime";
import { freeze_clock, unfreeze_clock } from "battletime/preview";
import face from "./face.tsx";

const NO_WEATHER = current_weather.value;

function shown(): string {
  return document.body.textContent ?? "";
}

describe("terminal", () => {
  beforeEach(() => {
    freeze_clock(new Date(2026, 9, 11, 14, 8, 30).getTime());
    const Face = face.screens.main;
    render(<Face />);
  });

  afterEach(() => {
    cleanup();
    unfreeze_clock();
    battery_percent.value = 80;
    is_charging.value = false;
    is_heart_rate_running.value = false;
    heart_rate.value = 0;
    is_bluetooth_enabled.value = true;
    is_phone_connected.value = true;
    unread_notifications.value = 0;
    settings.uses_24_hour_clock.value = true;
    current_weather.value = NO_WEATHER;
  });

  it("shows the time on a 24 hour clock, and with PM on a 12 hour one", () => {
    expect(shown()).toContain("14:08:30");
    act(() => { settings.uses_24_hour_clock.value = false; });
    expect(shown()).toContain("02:08:30 PM");
  });

  it("shows a notify line only while notifications are unread", () => {
    expect(shown()).not.toContain("[1]+ Notify");
    act(() => { unread_notifications.value = 2; });
    expect(shown()).toContain("[1]+ Notify");
  });

  it("shows the battery at every level, and says when it is charging", () => {
    for (const percent of [3, 10, 18, 30, 90]) {
      act(() => { battery_percent.value = percent; });
      expect(shown()).toContain(`${percent}%`);
    }
    act(() => { is_charging.value = true; });
    expect(shown()).toContain("90% Charging");
  });

  it("shows the heart rate only while it is measured", () => {
    expect(shown()).not.toContain("bpm");
    act(() => {
      heart_rate.value = 72;
      is_heart_rate_running.value = true;
    });
    expect(shown()).toContain("72 bpm");
  });

  it("shows dashes until the phone sends the weather", () => {
    expect(shown()).toContain("[WTHR]---");
    act(() => { current_weather.value = { ...NO_WEATHER, temperature: 21, received_at: 1 }; });
    expect(shown()).not.toContain("[WTHR]---");
  });

  it("says whether Bluetooth is off, connected or disconnected", () => {
    expect(shown()).toContain("Connected");
    act(() => { is_phone_connected.value = false; });
    expect(shown()).toContain("Disconnected");
    act(() => { is_bluetooth_enabled.value = false; });
    expect(shown()).toContain("Disabled");
  });
});
