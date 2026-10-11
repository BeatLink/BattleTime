// Renders every screen of every app, so a port that throws while drawing fails here.

import { cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it } from "vitest";
import type { App } from "neotime";

const modules = import.meta.glob<{ default: App }>("../*/app.tsx", { eager: true });

afterEach(cleanup);

describe.each(Object.entries(modules))("%s", (_path, module) => {
  const app = module.default;

  it("declares itself as an app", () => {
    expect(app.id).toMatch(/^[a-z0-9-]+$/);
    expect(app.kind).toBe("app");
    expect(app.screens.main).toBeTypeOf("function");
  });

  it.each(Object.keys(app.screens))("draws its %s screen", (screen_name) => {
    const ScreenComponent = app.screens[screen_name];
    const { container } = render(<ScreenComponent />);
    expect(container.querySelector("[data-widget=screen]")).not.toBeNull();
  });
});
