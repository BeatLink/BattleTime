// The simulator pane's webview: draws the simulator's frames and turns pointer input into touches.

(function () {
  const vscode = acquireVsCodeApi();
  const SCREEN_SIZE = 240;
  const MAX_LOG_LINES = 200;

  const screen = document.getElementById("screen");
  const context = screen.getContext("2d");
  const status = document.getElementById("status");
  const errors = document.getElementById("errors");
  const log = document.getElementById("log");

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
  // Messages from the extension
  // ==========================================================================

  window.addEventListener("message", (event) => {
    const message = event.data;
    switch (message.type) {
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
