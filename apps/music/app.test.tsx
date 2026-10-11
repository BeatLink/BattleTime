import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { advance_clock, deliver, effect_log, freeze_clock, install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import music from "./app.tsx";

afterEach(cleanup);

describe("music", () => {
  beforeAll(() => {
    install_apps([music]);
    start_scheduler();
  });

  it("runs the position on while playing and holds it when paused", () => {
    freeze_clock(new Date(2026, 0, 1, 12, 0, 0).getTime());
    open(music);
    render(<OpenScreen />);
    act(() => { deliver("music", { is_playing: true, artist: "Some band", track: "Some song", position: 10000, length: 180000 }); });
    expect(screen.getByText("00:10/03:00")).toBeTruthy();
    act(() => { advance_clock(5000); });
    expect(screen.getByText("00:15/03:00")).toBeTruthy();
    effect_log.value = [];
    act(() => { screen.getAllByRole("button")[1].click(); });
    expect(effect_log.value.some((effect) => effect.kind === "music_control" && effect.command === "pause")).toBe(true);
    act(() => { advance_clock(5000); });
    expect(screen.getByText("00:15/03:00")).toBeTruthy();
  });
});
