import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { reset_persisted, HOUR, MINUTE } from "neotime";
import { advance_clock, deliver, effect_log, freeze_clock, install_apps, open, open_app, OpenScreen, start_scheduler, swipe } from "neotime/preview";
import music from "./app.tsx";

let stop_scheduler = () => {};

/** The music commands sent to the phone so far. */
function commands(): unknown[] {
  return effect_log.value.filter((effect) => effect.kind === "music_control").map((effect) => effect.command);
}

function report(fields: { is_playing?: boolean; position?: number; length?: number }) {
  act(() => {
    deliver("music", { is_playing: false, artist: "Some band", track: "Some song", position: 0, length: 180000, ...fields });
  });
}

describe("music", () => {
  beforeEach(() => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    reset_persisted(music);
    install_apps([music]);
    stop_scheduler = start_scheduler();
    effect_log.value = [];
    act(() => { deliver("music", { is_playing: false, artist: "", track: "", position: 0, length: 0 }); });
    open(music);
    render(<OpenScreen />);
  });

  afterEach(() => {
    cleanup();
    stop_scheduler();
    open(null);
  });

  it("asks the phone for its state on open and shows nothing playing until it answers", () => {
    expect(effect_log.value.some((effect) => effect.kind === "request_music_state")).toBe(true);
    expect(screen.getByText("Nothing playing")).toBeTruthy();
    expect(screen.getByText("--:--/--:--")).toBeTruthy();
    report({});
    expect(screen.getByText("Some band")).toBeTruthy();
    expect(screen.getByText("Some song")).toBeTruthy();
  });

  it("runs the position on while playing and holds it when paused", () => {
    report({ is_playing: true, position: 10000 });
    expect(screen.getByText("00:10/03:00")).toBeTruthy();
    act(() => advance_clock(5000));
    expect(screen.getByText("00:15/03:00")).toBeTruthy();
    act(() => screen.getAllByRole("button")[1].click());
    expect(commands()).toEqual(["pause"]);
    act(() => advance_clock(5000));
    expect(screen.getByText("00:15/03:00")).toBeTruthy();
    act(() => screen.getAllByRole("button")[1].click());
    expect(commands()).toEqual(["pause", "play"]);
    act(() => advance_clock(5000));
    expect(screen.getByText("00:20/03:00")).toBeTruthy();
  });

  it("stops the position at the end of the track", () => {
    report({ is_playing: true, position: 175000 });
    act(() => advance_clock(30000));
    expect(screen.getByText("03:00/03:00")).toBeTruthy();
  });

  it("shows hours for long tracks and endless for very long ones", () => {
    report({ position: 30 * MINUTE, length: 2 * HOUR });
    expect(screen.getByText("0:30/2:00")).toBeTruthy();
    report({ length: 100 * HOUR });
    expect(screen.getByText("Inf/Inf")).toBeTruthy();
  });

  it("skips tracks with the side buttons and leaves left and right swipes alone", () => {
    act(() => screen.getAllByRole("button")[0].click());
    act(() => screen.getAllByRole("button")[2].click());
    act(() => swipe("left"));
    act(() => swipe("right"));
    expect(commands()).toEqual(["previous", "next"]);
  });

  it("turns the side buttons into volume buttons after a swipe up, until a swipe down", () => {
    act(() => swipe("up"));
    act(() => screen.getAllByRole("button")[0].click());
    act(() => screen.getAllByRole("button")[2].click());
    expect(commands()).toEqual(["volume_down", "volume_up"]);
    act(() => swipe("down"));
    expect(open_app.value).toBe(music);
    act(() => screen.getAllByRole("button")[0].click());
    expect(commands().at(-1)).toBe("previous");
    act(() => swipe("down"));
    expect(open_app.value).toBeNull();
  });
});
