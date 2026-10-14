import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "neotime";
import { bitmap_rows } from "neotime/widgets";
import { install_apps, open, OpenScreen } from "neotime/preview";
import app from "./app.tsx";

/** A 9 by 2 picture: the top row all set, the bottom row only its last pixel. */
const SMALL_CODE = String.fromCharCode(9, 2, 0xff, 0x80, 0x00, 0x80);

describe("qr-code", () => {
  beforeEach(() => {
    install_apps([app]);
  });

  afterEach(() => {
    cleanup();
    open(null);
    preview_files.value = {};
  });

  it("draws each set module of the code the phone sent", () => {
    preview_files.value = { "/qr/code": SMALL_CODE };
    open(app);
    const { container } = render(<OpenScreen />);

    expect(container.querySelectorAll("[data-widget='bitmap-pixel']")).toHaveLength(10);
  });

  it("shows the caption under the code", () => {
    preview_files.value = { "/qr/code": SMALL_CODE, "/qr/label": "Guest Wi-Fi" };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText("Guest Wi-Fi")).toBeTruthy();
  });

  it("asks for a code while there is none", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No QR code yet/)).toBeTruthy();
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
