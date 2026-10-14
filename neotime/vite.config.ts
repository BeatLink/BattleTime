import { defineConfig, type Plugin } from "vitest/config";
import preact from "@preact/preset-vite";

const signals_with_element_writes = decodeURIComponent(new URL("./sdk/neotime/src/signals.ts", import.meta.url).pathname);

// Loads the element-writes shim wherever `@preact/signals` is imported, except inside the shim itself.
function element_writes(): Plugin {
  return {
    name: "neotime-element-writes",
    enforce: "pre",
    resolveId(source, importer) {
      if (source !== "@preact/signals" || !importer || importer === signals_with_element_writes) return null;
      return signals_with_element_writes;
    },
  };
}

export default defineConfig({
  plugins: [element_writes(), preact()],
  test: {
    root: ".",
    environment: "jsdom",
    // The watch tests run in UTC, so the preview does too and both show the same times.
    env: { TZ: "UTC" },
    include: ["apps/**/*.test.tsx", "faces/**/*.test.tsx", "shell/**/*.test.tsx", "sdk/**/*.test.tsx"],
    exclude: ["**/node_modules/**"],
    coverage: {
      provider: "v8",
      include: ["apps/**/*.tsx", "faces/**/*.tsx", "shell/**/*.tsx", "sdk/neotime/src/**"],
      exclude: ["**/*.test.tsx", "**/tests/**"],
      reporter: ["text-summary", "text", "lcov"],
      reportsDirectory: "target/coverage-typescript",
      // Raise these whenever coverage rises, so it never slips back.
      thresholds: { lines: 95, statements: 95, branches: 90, functions: 93 },
    },
  },
});
