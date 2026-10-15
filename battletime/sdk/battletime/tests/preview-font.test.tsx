// The preview draws text in the watch's own font, registered once with the page.

import { describe, expect, it } from "vitest";
import { install_preview_fonts, PREVIEW_FONT_STYLE_ID } from "../src/widgets.tsx";
import { LIGHT_FONT_WOFF, TEXT_FONT_WOFF } from "../src/preview_font.ts";

describe("the preview's font", () => {
  it("is registered once, in both weights", () => {
    install_preview_fonts();
    install_preview_fonts();
    const styles = document.querySelectorAll(`#${PREVIEW_FONT_STYLE_ID}`);
    expect(styles.length).toBe(1);
    const text = styles[0].textContent ?? "";
    expect(text).toContain("font-family: 'BattleTime';");
    expect(text).toContain("font-family: 'BattleTime Light';");
  });

  it("holds WOFF files", () => {
    for (const woff of [TEXT_FONT_WOFF, LIGHT_FONT_WOFF]) {
      expect(atob(woff).slice(0, 4)).toBe("wOFF");
    }
  });
});
