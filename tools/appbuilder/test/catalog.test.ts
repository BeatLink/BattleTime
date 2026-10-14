import * as path from "node:path";
import { describe, expect, it } from "vitest";

import { read_catalog, type CatalogEntry } from "../src/catalog";

const SDK_SOURCE = path.resolve(__dirname, "../../../neotime/sdk/neotime/src");
const catalog = read_catalog(SDK_SOURCE);

function find(entries: CatalogEntry[], name: string): CatalogEntry {
  const entry = entries.find((candidate) => candidate.name === name);
  if (!entry) throw new Error(`${name} is not in the catalog`);
  return entry;
}

describe("the catalog read from the SDK", () => {
  it("lists widgets with their module, props and whether they take children", () => {
    const label = find(catalog.widgets, "Label");
    expect(label.module).toBe("neotime/widgets");
    expect(label.takes_children).toBe(true);
    expect(label.parameters.map((parameter) => parameter.name)).toContain("text_align");
    expect(label.parameters.find((parameter) => parameter.name === "grow")?.documentation).toMatch(/spare space/);
    expect(label.parameters.every((parameter) => parameter.name !== "children")).toBe(true);
    expect(label.parameters.find((parameter) => parameter.name === "text_align")).toMatchObject({
      type: expect.not.stringContaining("undefined"),
      optional: true,
    });

    expect(find(catalog.widgets, "Icon").takes_children).toBe(false);
    expect(find(catalog.widgets, "Card").module).toBe("neotime/components");
  });

  it("leaves out helpers that are not widgets", () => {
    const names = catalog.widgets.map((widget) => widget.name);
    expect(names).not.toContain("css_color");
    expect(names).not.toContain("scroll_pages");
  });

  it("lists every event in EventMap with its fields", () => {
    const tick = find(catalog.events, "tick");
    expect(tick.module).toBe("neotime");
    expect(find(catalog.events, "heart_rate").parameters.map((field) => field.name)).toContain("beats_per_minute");
    expect(find(catalog.events, "open").parameters).toEqual([]);
  });

  it("lists the functions that emit effects, with their arguments", () => {
    const vibrate = find(catalog.effects, "vibrate");
    expect(vibrate.parameters).toEqual([{ name: "milliseconds", type: "number", optional: false, documentation: "" }]);
    expect(vibrate.documentation).toMatch(/motor/);
    expect(find(catalog.effects, "show_toast").parameters[0].name).toBe("text");
    expect(catalog.effects.map((effect) => effect.name)).not.toContain("format_number");
  });

  it("lists the signals the watch keeps up to date", () => {
    expect(catalog.values.map((value) => value.name)).toEqual(expect.arrayContaining(["battery_percent", "now", "steps_today"]));
    expect(catalog.values.map((value) => value.name)).not.toContain("SECOND");
  });
});
