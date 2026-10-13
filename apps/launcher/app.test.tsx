import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { define_app, installed_apps, launcher_groups, launcher_style, launcher_view } from "neotime";
import { icons } from "neotime/icons";
import { install_apps, open, open_app, OpenScreen, screen_stack, start_scheduler, swipe } from "neotime/preview";
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
    launcher_groups.value = [];
    launcher_style.value = "list";
    launcher_view.value = "list";
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

  it("lists every app, however many are installed", () => {
    installed_apps.value = Array.from({ length: 20 }, (_, place) => ({ id: `app-${place}`, name: `App ${place}`, icon: icons.game }));
    render(<OpenScreen />);
    expect(screen.getAllByRole("button")).toHaveLength(20);
    expect(screen.getByText("App 19")).toBeTruthy();
  });

  describe("with groups", () => {
    const names = () => screen.queryAllByRole("button").map((button) => button.textContent ?? "");

    beforeEach(() => {
      installed_apps.value = [
        { id: "snake", name: "Snake", icon: icons.game },
        { id: "dice", name: "Dice", icon: icons.dice },
        { id: "timer", name: "Timer", icon: icons.timer },
      ];
      launcher_groups.value = [
        { name: "Games", icon: icons.game, first: 0, count: 2 },
        { name: "Time", icon: icons.timer, first: 2, count: 1 },
      ];
    });

    it("shows folders, and a tapped folder's apps", () => {
      launcher_style.value = "folders";
      render(<OpenScreen />);
      expect(names()).toEqual([expect.stringContaining("Games"), expect.stringContaining("Time")]);

      act(() => screen.getByText("Time").click());

      expect(screen_stack.value.at(-1)).toBe("group");
      expect(names()).toEqual([expect.stringContaining("Timer")]);
    });

    it("heads each group in one list in sections", () => {
      launcher_style.value = "sections";
      render(<OpenScreen />);

      expect(screen.getByText("Games")).toBeTruthy();
      expect(screen.getByText("Time")).toBeTruthy();
      expect(names()).toHaveLength(3);
    });

    it("turns pages with left and right swipes, staying within the groups", () => {
      launcher_style.value = "pages";
      render(<OpenScreen />);
      // The launcher remembers the page it was left on, so go back to the first.
      act(() => swipe("right"));
      act(() => swipe("right"));
      expect(screen.getByText("1/2")).toBeTruthy();

      act(() => swipe("left"));
      act(() => swipe("left"));
      expect(screen.getByText("2/2")).toBeTruthy();
      expect(names()).toEqual([expect.stringContaining("Timer")]);

      act(() => swipe("right"));
      expect(names()).toHaveLength(2);
    });

    it("closes to the face on a swipe down, whatever its style", () => {
      for (const style of ["list", "pages"] as const) {
        launcher_style.value = style;
        open(launcher);
        render(<OpenScreen />);
        act(() => swipe("down"));
        expect(open_app.value).toBeNull();
        cleanup();
      }
    });

    it("draws a grid that still opens the app tapped", () => {
      launcher_view.value = "grid";
      render(<OpenScreen />);
      expect(names()).toHaveLength(3);
      act(() => screen.getByText("Timer").click());

      expect(open_app.value).toBe(timer);
    });
  });
});
