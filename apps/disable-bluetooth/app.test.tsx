import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { is_bluetooth_enabled } from "neotime";
import { effect_log, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import disable_bluetooth from "./app.tsx";

afterEach(cleanup);

describe("disable-bluetooth", () => {
  beforeAll(() => {
    install_apps([disable_bluetooth]);
    start_scheduler();
  });

  it("turns Bluetooth off, and on again once the system reports it off", () => {
    is_bluetooth_enabled.value = true;
    open(disable_bluetooth);
    render(<OpenScreen />);
    effect_log.value = [];
    act(() => { screen.getByText("Disable").click(); });
    expect(effect_log.value.at(-1)).toEqual({ kind: "set_bluetooth", on: false });

    act(() => { is_bluetooth_enabled.value = false; });
    expect(screen.getByText("Bluetooth is off")).toBeTruthy();
    act(() => { screen.getByText("Enable").click(); });
    expect(effect_log.value.at(-1)).toEqual({ kind: "set_bluetooth", on: true });
    is_bluetooth_enabled.value = true;
  });
});
