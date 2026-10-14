// Builds the window that Tauri serves, on the desktop and on Android, into dist/.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite';

const packageJson = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as { version: string };

export default defineConfig({
  // Relative paths, so the page loads from Tauri's own scheme as well as from a server.
  base: './',
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  define: {
    __APP_VERSION__: JSON.stringify(packageJson.version),
  },
  esbuild: {
    jsx: 'automatic',
    jsxImportSource: 'preact',
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // WebKitGTK and Android's WebView both run modern JavaScript.
    target: 'es2022',
  },
  server: {
    port: 5173,
    strictPort: true,
  },
});
