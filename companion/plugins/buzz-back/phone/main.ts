// Buzz back's phone side: answers each press on the watch with buzzes, and counts the presses.
// It runs in the companion's QuickJS, with `neotime`, timers, `fetch` and `console`; see docs/plugins.md.

declare const neotime: {
  on(event: "message" | "connect" | "disconnect" | "ui", handler: (data: any) => void): void;
  send(message: { t: string; [key: string]: unknown }): void;
  ui: { send(message: unknown): void };
  settings: { get(key: string): any; set(key: string, value: unknown): void };
};

const DEFAULT_BUZZES = 2;

function buzzes(): number {
  return neotime.settings.get("buzzes") ?? DEFAULT_BUZZES;
}

function presses(): number {
  return neotime.settings.get("presses") ?? 0;
}

// What the settings page shows.
function report() {
  neotime.ui.send({ buzzes: buzzes(), presses: presses() });
}

neotime.on("message", () => {
  neotime.settings.set("presses", presses() + 1);
  neotime.send({ t: "vibrate", n: buzzes() });
  report();
});

neotime.on("ui", (message: { buzzes?: number }) => {
  if (typeof message.buzzes === "number") {
    neotime.settings.set("buzzes", Math.min(5, Math.max(1, Math.round(message.buzzes))));
  }
  report();
});

neotime.on("connect", () => console.log("The watch is connected; press Buzz me on it."));
