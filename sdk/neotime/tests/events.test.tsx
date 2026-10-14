// Checks `on`: each event becomes the subscription the preview runs, lifecycle events chain in
// the order connected, and `at_most_every` spaces out the handler by the watch's clock.

import { afterEach, describe, expect, it } from "vitest";
import { define_app, on, signal, EVERY_DAY, MINUTE, SECOND, type EventName, type Subscription } from "neotime";
import { advance_clock, deliver, freeze_clock, install_apps, open, start_scheduler } from "neotime/preview";

function Blank() {
  return null;
}

/** Defines an app after `connect` has made its `on` calls. */
function app_after(connect: () => void) {
  connect();
  return define_app({ id: "events", name: "Events", icon: "", screens: { main: Blank } });
}

let stop_scheduler = () => {};

afterEach(() => {
  stop_scheduler();
  open(null);
  install_apps([]);
});

describe("on", () => {
  it("turns every event into the subscription the preview runs", () => {
    const handler = () => {};
    const ring_at = signal<number | null>(5000);
    const plain: EventName[] = [
      "steps",
      "step_goal",
      "notification",
      "call",
      "music",
      "weather",
      "navigation",
      "settings_change",
      "minute",
      "hour",
      "day",
      "phone_connects",
      "phone_disconnects",
      "wrist_raise",
      "shake",
      "wrist_tap",
      "wrist_double_tap",
      "still",
      "moving",
      "battery_low",
      "charging_starts",
      "charging_stops",
      "charging_completes",
    ];
    const app = app_after(() => {
      for (const event of plain) on(event, handler);
      on("tick", handler, { every: MINUTE });
      on("heart_rate", handler, { every: { minutes: 5 } });
      on("acceleration", handler, { every: 100, while_open: true });
      on("time", handler, { at: ring_at });
      on("time", handler, { at: 9000 });
      on("daily", handler, { at: { hour: 7, minute: 30 }, days: 0b11111 });
      on("daily", handler, { at: { hour: 8, minute: 0 } });
    });

    const shapes = app.subscriptions.map((subscription: Subscription) => {
      const { handler: _, ...rest } = subscription as Subscription & { when?: unknown; time_of_day?: unknown };
      if ("when" in rest) return { ...rest, when: (rest.when as { peek(): unknown }).peek() };
      return rest;
    });
    expect(shapes).toEqual([
      { kind: "steps", only_while_open: false },
      { kind: "step_goal", only_while_open: false },
      { kind: "notification", only_while_open: false },
      { kind: "call", only_while_open: false },
      { kind: "music", only_while_open: false },
      { kind: "weather", only_while_open: false },
      { kind: "navigation", only_while_open: false },
      { kind: "settings_change", only_while_open: false },
      { kind: "clock_change", unit: "minute", only_while_open: false },
      { kind: "clock_change", unit: "hour", only_while_open: false },
      { kind: "clock_change", unit: "day", only_while_open: false },
      { kind: "phone", phone: "connects", only_while_open: false },
      { kind: "phone", phone: "disconnects", only_while_open: false },
      { kind: "motion", motion: "wrist_raise", only_while_open: false },
      { kind: "motion", motion: "shake", only_while_open: false },
      { kind: "motion", motion: "tap", only_while_open: false },
      { kind: "motion", motion: "double_tap", only_while_open: false },
      { kind: "motion", motion: "still", only_while_open: false },
      { kind: "motion", motion: "moving", only_while_open: false },
      { kind: "battery", battery: "low", only_while_open: false },
      { kind: "battery", battery: "starts", only_while_open: false },
      { kind: "battery", battery: "stops", only_while_open: false },
      { kind: "battery", battery: "completes", only_while_open: false },
      { kind: "every", period: { milliseconds: MINUTE }, only_while_open: false },
      { kind: "heart_rate_reading", period: { minutes: 5 }, only_while_open: false },
      { kind: "acceleration", period: { milliseconds: 100 }, only_while_open: true },
      { kind: "at", when: 5000, only_while_open: false },
      { kind: "at", when: 9000, only_while_open: false },
      { kind: "daily", time_of_day: { hour: 7, minute: 30 }, days: 0b11111, only_while_open: false },
      { kind: "daily", time_of_day: { hour: 8, minute: 0 }, days: EVERY_DAY, only_while_open: false },
    ]);
  });

  it("hands each app only the listeners connected since the app before it", () => {
    const first = app_after(() => on("steps", () => {}));
    const second = app_after(() => on("music", () => {}));

    expect(first.subscriptions.map((subscription) => subscription.kind)).toEqual(["steps"]);
    expect(second.subscriptions.map((subscription) => subscription.kind)).toEqual(["music"]);
  });

  it("runs every lifecycle handler for an event, in the order connected", () => {
    const calls: string[] = [];
    const app = app_after(() => {
      on("open", () => calls.push("open 1"));
      on("open", () => calls.push("open 2"));
      on("close", () => calls.push("close"));
      on("screen_on", () => calls.push("screen on"));
      on("screen_off", () => calls.push("screen off"));
    });

    open(app);
    app.onScreenOff?.();
    app.onScreenOn?.();
    open(null);

    expect(calls).toEqual(["open 1", "open 2", "screen off", "screen on", "close"]);
  });

  it("skips a handler until at_most_every has passed since it last ran", () => {
    freeze_clock(1_000_000);
    let shakes = 0;
    const app = app_after(() =>
      on("shake", () => (shakes += 1), { while_open: true, at_most_every: SECOND }),
    );
    install_apps([app]);
    stop_scheduler = start_scheduler();
    open(app);

    const shake = () => deliver("motion", { kind: "shake" }, (subscription) => subscription.kind === "motion");
    shake();
    advance_clock(400);
    shake();
    advance_clock(600);
    shake();
    shake();

    expect(shakes).toBe(2);
  });

  it("takes at_most_every as a period too", () => {
    freeze_clock(2_000_000);
    let ticks = 0;
    const app = app_after(() => on("steps", () => (ticks += 1), { at_most_every: { seconds: 2 } }));
    install_apps([app]);
    stop_scheduler = start_scheduler();

    deliver("steps", { steps: 1 });
    advance_clock(SECOND);
    deliver("steps", { steps: 2 });
    advance_clock(SECOND);
    deliver("steps", { steps: 3 });

    expect(ticks).toBe(2);
  });
});
