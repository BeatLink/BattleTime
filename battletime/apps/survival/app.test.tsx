import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "battletime";
import { install_apps, open, OpenScreen, start_scheduler } from "battletime/preview";
import app from "./app.tsx";

/** A list file holding these slots and titles, as the phone writes it. */
function list_file(entries: [number, string][]): string {
  const entry_text = ([slot, title]: [number, string]) =>
    String.fromCharCode(slot, title.length) + title.padEnd(24, "\0");
  return String.fromCharCode(1, entries.length) + entries.map(entry_text).join("");
}

/** Ten guides, so the last two come from the second half of the list. */
const TEN_GUIDES: [number, string][] = [
  [0, "CPR"],
  [1, "Severe bleeding"],
  [2, "Choking"],
  [3, "Burns"],
  [4, "Broken bones"],
  [5, "Stroke"],
  [6, "Allergic reaction"],
  [7, "Hypothermia"],
  [9, "Lost in the wild"],
  [13, "Earthquake"],
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

  it("lists each guide by its title, past the first eight too", () => {
    preview_files.value = { "/survival/list": list_file(TEN_GUIDES) };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText("CPR")).toBeTruthy();
    expect(screen.getByText("Hypothermia")).toBeTruthy();
    expect(screen.getByText("Lost in the wild")).toBeTruthy();
    expect(screen.getByText("Earthquake")).toBeTruthy();
  });

  it("opens the guide tapped, read from its own slot", () => {
    preview_files.value = {
      "/survival/list": list_file(TEN_GUIDES),
      "/survival/13": "Drop, cover and hold on.\n",
    };
    open(app);
    render(<OpenScreen />);

    act(() => screen.getByText("Earthquake").click());

    expect(screen.getByText("Earthquake")).toBeTruthy();
    expect(screen.getByText("Drop, cover and hold on.")).toBeTruthy();
  });

  it("says when a listed guide's text is missing", () => {
    preview_files.value = { "/survival/list": list_file([[4, "Broken bones"]]) };
    open(app);
    render(<OpenScreen />);

    act(() => screen.getByText("Broken bones").click());

    expect(screen.getByText(/This guide is missing/)).toBeTruthy();
  });

  it("asks for guides while there are none", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No guides yet/)).toBeTruthy();
  });
});
