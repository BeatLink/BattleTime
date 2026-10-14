import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { preview_files } from "neotime";
import { install_apps, open, OpenScreen } from "neotime/preview";
import app from "./app.tsx";

describe("notes", () => {
  beforeEach(() => {
    install_apps([app]);
  });

  afterEach(() => {
    cleanup();
    open(null);
    preview_files.value = {};
  });

  it("shows each line of the note the phone sent", () => {
    preview_files.value = { "/notes/note.txt": "Shopping\n\n- Bread\n- Milk\n" };
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText("Shopping")).toBeTruthy();
    expect(screen.getByText("- Bread")).toBeTruthy();
    expect(screen.getByText("- Milk")).toBeTruthy();
  });

  it("uses the font the phone chose", () => {
    preview_files.value = { "/notes/note.txt": "Big", "/notes/font": "large\n" };
    open(app);
    const { container } = render(<OpenScreen />);

    const scroll = container.querySelector("[data-widget='scroll']") as HTMLElement;
    expect(scroll.style.fontSize).not.toBe("");
  });

  it("asks for a note while there is none", () => {
    open(app);
    render(<OpenScreen />);

    expect(screen.getByText(/No note yet/)).toBeTruthy();
  });
});
