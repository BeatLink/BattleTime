import { afterEach, describe, expect, it } from "vitest";
import { file_bytes, list_preview_folder, normalise_folder_path, preview_files } from "../src/index.ts";
import { icons, library_icon } from "../src/icons.ts";

describe("file bytes", () => {
  afterEach(() => {
    preview_files.value = {};
  });

  it("gives a file's first bytes with zeros past its end", () => {
    preview_files.value = { "/remote/buttons": String.fromCharCode(1, 3, 26, 32) };

    expect(file_bytes("/remote/buttons", 6)).toEqual([1, 3, 26, 32, 0, 0]);
    expect(file_bytes("/remote/buttons", 2)).toEqual([1, 3]);
  });

  it("gives zeros for a missing file", () => {
    expect(file_bytes("/remote/buttons", 3)).toEqual([0, 0, 0]);
  });
});

describe("preview folders", () => {
  const files = {
    "/notes/note.txt": "Packing list",
    "/apps/timer/image": "t".repeat(40),
    "/apps/dice/image": "d".repeat(30),
    "/apps/dice/state": String.fromCharCode(0xfe, 0x07),
    "/settings": "é€",
    "/a-file": "1234567",
  };

  it("write a path plainly from the top, never climbing above it", () => {
    expect(normalise_folder_path("")).toBe("/");
    expect(normalise_folder_path("//apps/./dice/..")).toBe("/apps");
    expect(normalise_folder_path("/../..")).toBe("/");
    expect(normalise_folder_path(`/${"a".repeat(64)}`)).toBeNull();
  });

  it("list folders first, then files, each by name, with a folder's size the sum of its files", () => {
    expect(list_preview_folder(files, "/")).toEqual({
      path: "/",
      entries: [
        { name: "apps", size: 72, is_folder: true },
        { name: "notes", size: 12, is_folder: true },
        { name: "a-file", size: 7, is_folder: false },
        { name: "settings", size: 4, is_folder: false },
      ],
    });
  });

  it("list only what lies inside a folder", () => {
    expect(list_preview_folder(files, "/apps/dice").entries).toEqual([
      { name: "image", size: 30, is_folder: false },
      { name: "state", size: 2, is_folder: false },
    ]);
    expect(list_preview_folder({ "/apps-old/x": "1" }, "/apps").entries).toEqual([]);
  });
});

describe("library icons by number", () => {
  it("number icons in the shared library's order", () => {
    expect(library_icon(0)).toEqual(icons.alarm);
    expect(library_icon(31)).toEqual(icons.phone);
    expect(library_icon(57)).toEqual(icons.weather);
    expect(library_icon(58)).toEqual(icons.none);
    expect(library_icon(59)).toEqual(icons.weather_night_clear);
    expect(library_icon(62)).toEqual(icons.info);
    expect(library_icon(63)).toEqual(icons.remote);
    expect(library_icon(64)).toEqual(icons.note);
    expect(library_icon(65)).toEqual(icons.qr_code);
    expect(library_icon(66)).toEqual(icons.phone_missed);
    expect(library_icon(71)).toEqual(icons.calendar);
  });

  it("give none outside the library", () => {
    expect(library_icon(72)).toEqual(icons.none);
    expect(library_icon(-1)).toEqual(icons.none);
  });
});
