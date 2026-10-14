import { act, cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "neotime";
import { bitmap_rows } from "neotime/widgets";
import { install_apps, open, OpenScreen, start_scheduler } from "neotime/preview";
import app from "./app.tsx";

/** A 9 by 2 picture: the top row all set, the bottom row only its last pixel. */
const SMALL_CODE = String.fromCharCode(9, 2, 0xff, 0x80, 0x00, 0x80);

/** A list file holding these slots and captions, as the phone writes it. */
function list_file(entries: [number, string][]): string {
  const entry_text = ([slot, caption]: [number, string]) =>
    String.fromCharCode(slot, caption.length) + caption.padEnd(24, "\0");
  return String.fromCharCode(1, entries.length) + entries.map(entry_text).join("");
}

let stop_scheduler = () => {};

describe("qr-code", () => {
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

  it("lists each code by its caption", () => {
    preview_files.value = { "/qr/list": list_file([[0, "Guest Wi-Fi"], [5, "example.com"]]) };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText("Guest Wi-Fi")).toBeTruthy();
    expect(screen.getByText("example.com")).toBeTruthy();
  });

  it("draws the code tapped, read from its own slot", () => {
    preview_files.value = {
      "/qr/list": list_file([[0, "Guest Wi-Fi"], [5, "example.com"]]),
      "/qr/5": SMALL_CODE,
    };
    open(app);
    const { container } = render(<OpenScreen />);

    act(() => screen.getByText("example.com").click());

    expect(container.querySelectorAll("[data-widget='bitmap-pixel']")).toHaveLength(10);
    expect(screen.getByText("example.com")).toBeTruthy();
  });

  it("asks for codes while there are none", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No QR codes yet/)).toBeTruthy();
  });
});

describe("bitmap_rows", () => {
  it("unpacks rows with the leftmost pixel in the high bit", () => {
    expect(bitmap_rows(SMALL_CODE)[1]).toEqual([false, false, false, false, false, false, false, false, true]);
  });

  it("gives nothing for a missing, oversized or short file", () => {
    expect(bitmap_rows("")).toEqual([]);
    expect(bitmap_rows(String.fromCharCode(121, 1))).toEqual([]);
    expect(bitmap_rows(String.fromCharCode(8, 2, 0xff))).toEqual([]);
  });
});
