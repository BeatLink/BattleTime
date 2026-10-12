import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { define_app, installed_apps } from "neotime";
import { icons } from "neotime/icons";
import { install_apps, open, open_app, OpenScreen, start_scheduler } from "neotime/preview";
import { Screen } from "neotime/widgets";
import launcher from "./app.tsx";

let stop_scheduler = () => {};

const timer = define_app({ id: "timer", name: "Timer", icon: icons.timer, screens: { main: () => <Screen /> } });

describe("launcher", () => {
  beforeEach(() => {
    install_apps([launcher, timer]);
    stop_scheduler = start_scheduler();
    open(launcher);
  });

  afterEach(() => {
    cleanup();
    installed_apps.value = [];
    stop_scheduler();
    open(null);
  });

  it("lists every installed app by name", () => {
    installed_apps.value = [
      { id: "timer", name: "Timer", icon: icons.timer },
      { id: "dice", name: "Dice", icon: icons.dice },
    ];
    render(<OpenScreen />);
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      expect.stringContaining("Timer"),
      expect.stringContaining("Dice"),
    ]);
  });

  it("opens the app whose row is tapped", () => {
    installed_apps.value = [{ id: "timer", name: "Timer", icon: icons.timer }];
    render(<OpenScreen />);
    act(() => screen.getByText("Timer").click());
    expect(open_app.value).toBe(timer);
  });

  it("has rows for the first sixteen apps", () => {
    installed_apps.value = Array.from({ length: 20 }, (_, place) => ({ id: `app-${place}`, name: `App ${place}`, icon: icons.game }));
    render(<OpenScreen />);
    expect(screen.getAllByRole("button")).toHaveLength(16);
    expect(screen.queryByText("App 16")).toBeNull();
  });
});
