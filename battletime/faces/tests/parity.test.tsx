// Checks that every watch face shows the same texts in the preview as on the watch.

import { describe } from "vitest";
import type { App } from "battletime";
import { describe_parity } from "../../sdk/battletime/tests/parity.tsx";

const modules = import.meta.glob<{ default: App }>("../*/face.tsx", { eager: true });

describe("faces in the preview and on the watch", () => describe_parity("face", modules));
