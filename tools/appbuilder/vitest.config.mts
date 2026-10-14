import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // TypeScript is plain CommonJS with no source map, so it loads as it is.
    server: { deps: { external: [/typescript/] } },
    include: ["test/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts"],
      // The VS Code glue needs a running editor, so it is checked by hand, as the README says.
      exclude: ["src/extension.ts", "src/panel.ts"],
      // Raise these whenever coverage rises.
      thresholds: { lines: 100, functions: 100, statements: 100, branches: 94 },
    },
  },
});
