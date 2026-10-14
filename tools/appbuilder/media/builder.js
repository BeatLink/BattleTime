// The builder's webview: draws the simulator's frames, turns pointer input into touches, and lists the palette.

(function () {
  const vscode = acquireVsCodeApi();
  const SCREEN_SIZE = 240;
  const SECTION_TITLES = { widgets: "Widgets", events: "Events", effects: "Effects", values: "Values" };
  const KINDS = { widgets: "widget", events: "event", effects: "effect", values: "value" };

  const screen = document.getElementById("screen");
  const context = screen.getContext("2d");
  const status = document.getElementById("status");
  const errors = document.getElementById("errors");
  const log = document.getElementById("log");
  const search = document.getElementById("search");
  const sections = document.getElementById("sections");
  let catalog = null;

  // ==========================================================================
  // The screen
  // ==========================================================================

  function draw(pixels) {
    const bytes = new Uint8ClampedArray(pixels.buffer ?? pixels, pixels.byteOffset ?? 0, SCREEN_SIZE * SCREEN_SIZE * 4);
    context.putImageData(new ImageData(bytes, SCREEN_SIZE, SCREEN_SIZE), 0, 0);
  }

  /** Where a pointer is on the watch's 240x240 screen, however large the canvas is drawn. */
  function screen_point(event) {
    const box = screen.getBoundingClientRect();
    const clamp = (value) => Math.min(SCREEN_SIZE - 1, Math.max(0, value));
    return {
      x: clamp(((event.clientX - box.left) * SCREEN_SIZE) / box.width),
      y: clamp(((event.clientY - box.top) * SCREEN_SIZE) / box.height),
    };
  }

  let pressed = false;
  screen.addEventListener("pointerdown", (event) => {
    pressed = true;
    screen.setPointerCapture(event.pointerId);
    vscode.postMessage({ type: "pointer", action: "press", ...screen_point(event) });
  });
  screen.addEventListener("pointermove", (event) => {
    if (pressed) vscode.postMessage({ type: "pointer", action: "move", ...screen_point(event) });
  });
  screen.addEventListener("pointerup", (event) => {
    if (!pressed) return;
    pressed = false;
    vscode.postMessage({ type: "pointer", action: "release", ...screen_point(event) });
  });

  for (const id of ["button", "theme", "restart"]) {
    document.getElementById(id).addEventListener("click", () => vscode.postMessage({ type: id }));
  }

  // ==========================================================================
  // The palette
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
    if (!catalog) return;
    const wanted = search.value.trim().toLowerCase();
    sections.replaceChildren();
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

  const MAX_LOG_LINES = 200;

  window.addEventListener("message", (event) => {
    const message = event.data;
    switch (message.type) {
      case "catalog":
        catalog = message.catalog;
        show_palette();
        break;
      case "frame":
        draw(message.pixels);
        break;
      case "motor":
        screen.classList.toggle("vibrating", message.running);
        break;
      case "status":
        status.textContent = message.text;
        break;
      case "errors":
        errors.replaceChildren(
          ...message.errors.map((error) => {
            const item = document.createElement("li");
            item.textContent = `line ${error.line}: ${error.message}`;
            return item;
          }),
        );
        break;
      case "log": {
        const lines = (log.textContent + message.text + "\n").split("\n");
        log.textContent = lines.slice(-MAX_LOG_LINES - 1).join("\n");
        log.scrollTop = log.scrollHeight;
        break;
      }
    }
  });

  vscode.postMessage({ type: "ready" });
})();
