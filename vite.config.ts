import { defineConfig } from "vitest/config";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  test: {
    root: ".",
    environment: "jsdom",
    include: ["apps/**/*.test.tsx", "faces/**/*.test.tsx", "sdk/**/*.test.tsx"],
    exclude: ["**/node_modules/**"],
  },
});
