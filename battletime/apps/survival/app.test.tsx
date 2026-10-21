import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "battletime";
import { install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

/** A list file holding these slots, topics and titles, as the phone writes it. */
function list_file(entries: [number, number, string][]): string {
  const entry_text = ([slot, topic, title]: [number, number, string]) =>
    String.fromCharCode(slot, topic, title.length) + title.padEnd(23, "\0");
  return String.fromCharCode(2, entries.length) + entries.map(entry_text).join("");
}

/** Eighteen guides over four topics, so the last ones come from the list's third part. */
const GUIDES: [number, number, string][] = [
  [0, 0, "CPR"],
  [1, 0, "Severe bleeding"],
  [2, 0, "Choking"],
  [3, 0, "Burns"],
  [4, 0, "Broken bones"],
  [5, 0, "Stroke"],
  [6, 0, "Allergic reaction"],
  [7, 1, "Hypothermia"],
  [8, 1, "Heat illness"],
  [9, 1, "Lost in the wild"],
  [10, 1, "Finding water"],
  [11, 1, "Shelter and fire"],
  [12, 1, "Signalling for help"],
  [13, 2, "Earthquake"],
  [14, 3, "Protesting safely"],
  [15, 3, "Nonviolent action"],
  [16, 3, "De-escalating violence"],
  [18, 3, "Police and arrest"],
];

let stop_scheduler = () => {};

describe("survival", () => {
  beforeEach(() => {
    install_apps([app]);
    stop_scheduler = start_scheduler();
  });

  afterEach(() => {
    stop_scheduler();
    cleanup();
    open(null);
    preview_files.value = {};
  });

  it("lists the topics that have guides, and a topic's guides, past the first sixteen too", () => {
    preview_files.value = { "/survival/list": list_file(GUIDES.filter(([, topic]) => topic !== 2)) };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText("First aid")).toBeTruthy();
    expect(screen.getByText("Personal safety")).toBeTruthy();
    expect(screen.queryByText("Disasters")).toBeNull();

    act(() => screen.getByText("Personal safety").click());

    expect(screen.getByText("Protesting safely")).toBeTruthy();
    expect(screen.getByText("Police and arrest")).toBeTruthy();
    expect(screen.queryByText("CPR")).toBeNull();
  });

  it("opens the guide tapped, read from its own slot", () => {
    preview_files.value = {
      "/survival/list": list_file(GUIDES),
      "/survival/18": "Stay calm.\n",
    };
    open(app);
    render(<OpenScreen />);

    act(() => screen.getByText("Personal safety").click());
    act(() => screen.getByText("Police and arrest").click());

    expect(screen.getByText("Police and arrest")).toBeTruthy();
    expect(screen.getByText("Stay calm.")).toBeTruthy();
  });

  it("says when a listed guide's text is missing", () => {
    preview_files.value = { "/survival/list": list_file([[4, 0, "Broken bones"]]) };
    open(app);
    render(<OpenScreen />);

    act(() => screen.getByText("First aid").click());
    act(() => screen.getByText("Broken bones").click());

    expect(screen.getByText(/This guide is missing/)).toBeTruthy();
  });

  it("shows nothing from a list in the first format", () => {
    preview_files.value = { "/survival/list": String.fromCharCode(1, 1, 0, 3) + "CPR".padEnd(24, "\0") };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No guides yet/)).toBeTruthy();
  });

  it("asks for guides while there are none", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No guides yet/)).toBeTruthy();
  });
});
