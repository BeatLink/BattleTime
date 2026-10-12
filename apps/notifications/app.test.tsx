import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { define_app, notifications, notify, unread_notifications } from "neotime";
import {
  clear_notifications,
  deliver,
  install_apps,
  open,
  OpenScreen,
  run_app_handler,
  screen_stack,
  start_scheduler,
} from "neotime/preview";
import { icons } from "neotime/icons";
import { Screen } from "neotime/widgets";
import notifications_app from "./app.tsx";

let stop_scheduler = () => {};

const timer = define_app({ id: "timer", name: "Timer", icon: icons.timer, screens: { main: () => <Screen /> } });

function from_phone(id: number, title: string, body = "On my way") {
  act(() => deliver("notification", { id, app: "Messages", title, body }));
}

describe("notifications", () => {
  beforeEach(() => {
    clear_notifications();
    install_apps([notifications_app, timer]);
    stop_scheduler = start_scheduler();
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
    clear_notifications();
  });

  it("says so when there are none", () => {
    open(notifications_app);
    render(<OpenScreen />);
    expect(screen.getByText("No notifications")).toBeTruthy();
  });

  it("lists the newest first and marks them read when it opens", () => {
    from_phone(1, "Sam");
    from_phone(2, "Alex");
    expect(unread_notifications.value).toBe(2);

    open(notifications_app);
    render(<OpenScreen />);

    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      expect.stringContaining("Alex"),
      expect.stringContaining("Sam"),
    ]);
    expect(unread_notifications.value).toBe(0);
  });

  it("reads one in full and dismisses it", () => {
    from_phone(1, "Sam", "On my way");
    from_phone(2, "Alex", "Running late");
    open(notifications_app);
    render(<OpenScreen />);

    act(() => screen.getByText("Sam").click());
    expect(screen_stack.value).toEqual(["main", "message"]);
    expect(screen.getByText("On my way")).toBeTruthy();

    act(() => screen.getByText("Dismiss").click());
    expect(screen_stack.value).toEqual(["main"]);
    expect(notifications.value.map((kept) => kept.title)).toEqual(["Alex"]);
  });

  it("keeps an app's notification with the app's name", () => {
    run_app_handler(timer, () => notify("Timer", "Time is up"), undefined);

    expect(notifications.value[0]).toMatchObject({ app: "Timer", title: "Timer", body: "Time is up" });
    expect(notifications.value[0].id).toBeLessThan(0);
  });

  it("keeps only the latest five, and a repeated id replaces the one it names", () => {
    for (let id = 1; id <= 6; id++) from_phone(id, `Number ${id}`);
    from_phone(4, "Number 4, edited");

    expect(notifications.value.map((kept) => kept.title)).toEqual([
      "Number 4, edited",
      "Number 6",
      "Number 5",
      "Number 3",
      "Number 2",
    ]);
  });
});
