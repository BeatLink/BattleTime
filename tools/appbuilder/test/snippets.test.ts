import { describe, expect, it } from "vitest";

import type { CatalogEntry, Parameter } from "../src/catalog";
import { add_import, import_for, snippet_for, type TextChange } from "../src/snippets";

// ============================================================================
// Helpers
// ============================================================================

function parameter(name: string, optional = false): Parameter {
  return { name, type: "number", optional, documentation: "" };
}

function entry(kind: CatalogEntry["kind"], name: string, parameters: Parameter[] = [], takes_children = false): CatalogEntry {
  const module = kind === "widget" ? "battletime/widgets" : "battletime";
  return { kind, name, module, documentation: "", parameters, takes_children };
}

function apply(source: string, change: TextChange | undefined): string {
  if (!change) return source;
  return source.slice(0, change.start) + change.text + source.slice(change.end);
}

// ============================================================================
// Snippets
// ============================================================================

describe("snippets", () => {
  it("give a widget its required props, and children when it takes them", () => {
    expect(snippet_for(entry("widget", "Label", [parameter("grow", true)], true))).toBe("<Label>${1}</Label>$0");
    expect(snippet_for(entry("widget", "Bar", [parameter("value"), parameter("color", true)]))).toBe("<Bar value={${1:value}} />$0");
  });

  it("connect an event, with the event argument only when it has fields", () => {
    expect(snippet_for(entry("event", "tick", [parameter("now")]))).toBe('on("tick", (event) => {\n\t$0\n});');
    expect(snippet_for(entry("event", "open"))).toBe('on("open", () => {\n\t$0\n});');
  });

  it("call an effect with its required arguments", () => {
    expect(snippet_for(entry("effect", "notify", [parameter("title"), parameter("body")]))).toBe("notify(${1:title}, ${2:body});$0");
    expect(snippet_for(entry("effect", "format", [parameter("value"), parameter("trim", true)]))).toBe("format(${1:value});$0");
    expect(snippet_for(entry("effect", "back"))).toBe("back();$0");
  });

  it("read a value through its signal", () => {
    expect(snippet_for(entry("value", "battery_percent"))).toBe("battery_percent.value$0");
  });

  it("escape snippet syntax in placeholder names", () => {
    expect(snippet_for(entry("effect", "odd", [parameter("a$b}")]))).toBe("odd(${1:a\\$b\\}});$0");
  });

  it("import on for events and the entry's own name otherwise", () => {
    expect(import_for(entry("event", "tick"))).toEqual({ name: "on", module: "battletime" });
    expect(import_for(entry("widget", "Label"))).toEqual({ name: "Label", module: "battletime/widgets" });
  });
});

// ============================================================================
// Imports
// ============================================================================

describe("adding an import", () => {
  const app = 'import { define_app } from "battletime";\nimport { Label, Screen } from "battletime/widgets";\n\nconst x = 1;\n';

  it("joins an existing one-line import, in order", () => {
    expect(apply(app, add_import(app, "Icon", "battletime/widgets"))).toContain('import { Icon, Label, Screen } from "battletime/widgets";');
  });

  it("leaves the source alone when the name is already imported, even as a type or renamed", () => {
    expect(add_import(app, "Label", "battletime/widgets")).toBeUndefined();
    expect(add_import('import { type Foo } from "battletime";', "Foo", "battletime")).toBeUndefined();
    expect(add_import('import { on as listen } from "battletime";', "on", "battletime")).toBeUndefined();
  });

  it("keeps a multi-line import multi-line", () => {
    const source = 'import {\n  define_app,\n  type HeartRateEvent,\n  vibrate,\n} from "battletime";\n';
    expect(apply(source, add_import(source, "on", "battletime"))).toBe(
      'import {\n  define_app,\n  type HeartRateEvent,\n  on,\n  vibrate,\n} from "battletime";\n',
    );
  });

  it("does not mistake one module for another that starts the same way", () => {
    const result = apply(app, add_import(app, "vibrate", "battletime"));
    expect(result).toContain('import { define_app, vibrate } from "battletime";');
    expect(result).toContain('import { Label, Screen } from "battletime/widgets";');
  });

  it("adds a new import after the last one", () => {
    expect(apply(app, add_import(app, "Card", "battletime/components"))).toBe(
      'import { define_app } from "battletime";\nimport { Label, Screen } from "battletime/widgets";\nimport { Card } from "battletime/components";\n\nconst x = 1;\n',
    );
  });

  it("puts the first import after the file's opening comments", () => {
    const source = "// An app.\n// Second line.\n\nconst x = 1;\n";
    expect(apply(source, add_import(source, "on", "battletime"))).toBe(
      '// An app.\n// Second line.\n\nimport { on } from "battletime";\n\nconst x = 1;\n',
    );
  });
});
