// Buzz back's phone side: answers each press on the watch with buzzes, and counts the presses.
// It runs in the companion's QuickJS, with `battletime`, timers, `fetch` and `console`; see docs/plugins.md.

declare const battletime: {
  on(event: "message" | "connect" | "disconnect" | "ui", handler: (data: any) => void): void;
  send(message: { t: string; [key: string]: unknown }): void;
  ui: { send(message: unknown): void };
  settings: { get(key: string): any; set(key: string, value: unknown): void };
};

const DEFAULT_BUZZES = 2;

function buzzes(): number {
  return battletime.settings.get("buzzes") ?? DEFAULT_BUZZES;
}

function presses(): number {
  return battletime.settings.get("presses") ?? 0;
}

// What the settings page shows.
function report() {
  battletime.ui.send({ buzzes: buzzes(), presses: presses() });
}

battletime.on("message", () => {
  battletime.settings.set("presses", presses() + 1);
  battletime.send({ t: "vibrate", n: buzzes() });
  report();
});

battletime.on("ui", (message: { buzzes?: number }) => {
  if (typeof message.buzzes === "number") {
    battletime.settings.set("buzzes", Math.min(5, Math.max(1, Math.round(message.buzzes))));
  }
  report();
});

battletime.on("connect", () => console.log("The watch is connected; press Buzz me on it."));
