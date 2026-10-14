// The palette pane's webview: lists the widgets, events, effects and values, and asks for one to be inserted.

(function () {
  const vscode = acquireVsCodeApi();
  const SECTION_TITLES = { widgets: "Widgets", events: "Events", effects: "Effects", values: "Values" };
  const KINDS = { widgets: "widget", events: "event", effects: "effect", values: "value" };

  const target = document.getElementById("target");
  const search = document.getElementById("search");
  const sections = document.getElementById("sections");
  let catalog = null;

  // ==========================================================================
  // Entries
  // ==========================================================================

  function signature(kind, entry) {
    const required = entry.parameters.filter((parameter) => !parameter.optional).map((parameter) => parameter.name);
    if (kind === "events") return `on("${entry.name}")`;
    if (kind === "effects") return `${entry.name}(${required.join(", ")})`;
    if (kind === "values") return `${entry.name}.value`;
    return `<${entry.name}${required.map((name) => ` ${name}`).join("")}>`;
  }

  function entry_element(kind, entry) {
    const item = document.createElement("li");
    const insert = document.createElement("button");
    insert.className = "entry";
    insert.textContent = signature(kind, entry);
    insert.title = entry.documentation || `Insert ${entry.name} at the cursor`;
    insert.addEventListener("click", () => vscode.postMessage({ type: "insert", kind: KINDS[kind], name: entry.name }));
    item.append(insert);

    if (entry.documentation || entry.parameters.length > 0) {
      const details = document.createElement("details");
      const summary = document.createElement("summary");
      summary.textContent = entry.documentation || (kind === "events" ? "Fields" : kind === "widgets" ? "Props" : "Arguments");
      details.append(summary);
      const list = document.createElement("dl");
      for (const parameter of entry.parameters) {
        const term = document.createElement("dt");
        term.textContent = `${parameter.name}${parameter.optional ? "?" : ""}: ${parameter.type}`;
        list.append(term);
        if (parameter.documentation) {
          const description = document.createElement("dd");
          description.textContent = parameter.documentation;
          list.append(description);
        }
      }
      details.append(list);
      item.append(details);
    }
    return item;
  }

  function show_palette() {
    sections.replaceChildren();
    if (!catalog) return;
    const wanted = search.value.trim().toLowerCase();
    for (const [kind, title] of Object.entries(SECTION_TITLES)) {
      const entries = catalog[kind].filter(
        (entry) => !wanted || entry.name.toLowerCase().includes(wanted) || entry.documentation.toLowerCase().includes(wanted),
      );
      if (entries.length === 0) continue;
      const section = document.createElement("details");
      section.open = wanted.length > 0 || kind === "widgets";
      const heading = document.createElement("summary");
      heading.className = "section";
      heading.textContent = `${title} (${entries.length})`;
      const list = document.createElement("ul");
      list.append(...entries.map((entry) => entry_element(kind, entry)));
      section.append(heading, list);
      sections.append(section);
    }
  }

  search.addEventListener("input", show_palette);

  // ==========================================================================
  // Messages from the extension
  // ==========================================================================

  window.addEventListener("message", (event) => {
    const message = event.data;
    if (message.type !== "state") return;
    catalog = message.catalog ?? null;
    target.textContent = message.target ? `Inserts into ${message.target}` : "Open an app's .tsx file to fill the palette.";
    search.hidden = !catalog;
    show_palette();
  });

  vscode.postMessage({ type: "ready" });
})();
