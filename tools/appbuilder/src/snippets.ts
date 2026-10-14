// What a palette entry puts into the app's source: the snippet at the cursor and the import it needs.

import type { CatalogEntry } from "./catalog";

// ============================================================================
// Snippets
// ============================================================================

/** The text to insert, in VS Code's snippet syntax, with a tab stop for each argument still to fill. */
export function snippet_for(entry: CatalogEntry): string {
  switch (entry.kind) {
    case "widget":
      return widget_snippet(entry);
    case "event": {
      const handler_argument = entry.parameters.length > 0 ? "event" : "";
      return `on("${entry.name}", (${handler_argument}) => {\n\t$0\n});`;
    }
    case "effect": {
      const required = entry.parameters.filter((parameter) => !parameter.optional);
      const arguments_text = required.map((parameter, place) => `\${${place + 1}:${escape(parameter.name)}}`).join(", ");
      return `${entry.name}(${arguments_text});$0`;
    }
    case "value":
      return `${entry.name}.value$0`;
  }
}

function widget_snippet(entry: CatalogEntry): string {
  const required = entry.parameters.filter((parameter) => !parameter.optional);
  const props = required.map((parameter, place) => ` ${parameter.name}={\${${place + 1}:${escape(parameter.name)}}}`).join("");
  const last_stop = required.length + 1;
  if (entry.takes_children) return `<${entry.name}${props}>\${${last_stop}}</${entry.name}>$0`;
  return `<${entry.name}${props} />$0`;
}

/** Keeps a placeholder's text from being read as snippet syntax. */
function escape(text: string): string {
  return text.replace(/[$}\\]/g, (character) => `\\${character}`);
}

/** The name an entry needs imported, and from where: events need `on`. */
export function import_for(entry: CatalogEntry): { name: string; module: string } {
  return { name: entry.kind === "event" ? "on" : entry.name, module: entry.module };
}

// ============================================================================
// Imports
// ============================================================================

/** A change to the source: replace `[start, end)` with `text`. */
export interface TextChange {
  start: number;
  end: number;
  text: string;
}

/** The change that imports `name` from `module`, or nothing when the source already imports it. */
export function add_import(source: string, name: string, module: string): TextChange | undefined {
  const quoted_module = module.replace(/[/.]/g, (character) => `\\${character}`);
  const existing = new RegExp(`import\\s*\\{([^}]*)\\}\\s*from\\s*["']${quoted_module}["']`).exec(source);

  if (existing) {
    const names = existing[1]
      .split(",")
      .map((part) => part.trim())
      .filter((part) => part.length > 0);
    if (names.some((part) => part.replace(/^type\s+/, "").split(/\s+as\s+/)[0] === name)) return undefined;
    names.push(name);
    names.sort((first, second) => sort_key(first).localeCompare(sort_key(second)));

    const is_multiline = existing[1].includes("\n");
    const list = is_multiline ? `\n${names.map((part) => `  ${part},`).join("\n")}\n` : ` ${names.join(", ")} `;
    const start = existing.index + existing[0].indexOf("{") + 1;
    return { start, end: start + existing[1].length, text: list };
  }

  const line = `import { ${name} } from "${module}";\n`;
  const imports = [...source.matchAll(/^import[\s\S]*?from\s*["'][^"']+["'];?[^\n]*\n/gm)];
  if (imports.length > 0) {
    const last = imports[imports.length - 1];
    const end = last.index! + last[0].length;
    return { start: end, end, text: line };
  }
  const after_comments = /^(?:\s*\/\/[^\n]*\n)*\s*/.exec(source)![0].length;
  return { start: after_comments, end: after_comments, text: `${line}\n` };
}

/** Sorts `type Name` among the plain names by the name itself. */
function sort_key(part: string): string {
  return part.replace(/^type\s+/, "").toLowerCase();
}
