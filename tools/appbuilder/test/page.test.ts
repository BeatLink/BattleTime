import { describe, expect, it } from "vitest";

import { page_html } from "../src/page";

describe("a webview page", () => {
  const html = page_html({
    title: "Palette",
    csp_source: "vscode-resource:",
    nonce: "abc123",
    style_uris: ["common.css", "palette.css"],
    script_uri: "palette.js",
    body: "<main></main>",
  });

  it("lets only its own script run and only the webview serve styles", () => {
    expect(html).toContain("default-src 'none'; style-src vscode-resource:; script-src 'nonce-abc123';");
    expect(html).toContain('<script nonce="abc123" src="palette.js"></script>');
  });

  it("links every stylesheet in order and holds the body", () => {
    expect(html.indexOf("common.css")).toBeLessThan(html.indexOf("palette.css"));
    expect(html).toContain("<title>Palette</title>");
    expect(html).toContain("<body>\n<main></main>");
  });
});
