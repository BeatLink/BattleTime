// Checks that every app's screens show the same texts in the preview as on the watch.

import { describe } from "vitest";
import type { App } from "battletime";
import { describe_parity } from "../../sdk/battletime/tests/parity.tsx";

const modules = import.meta.glob<{ default: App }>("../*/app.tsx", { eager: true });

describe("apps in the preview and on the watch", () => describe_parity("app", modules));
