const js = require('@eslint/js');
const { defineConfig } = require('eslint/config');
const globals = require('globals');
const reactHooks = require('eslint-plugin-react-hooks');
const tseslint = require('typescript-eslint');

module.exports = defineConfig([
  {
    ignores: ['dist/*', 'build/*', 'coverage/*', 'node_modules/*', 'tauri/gen/*', 'target/*', '.claude/*', 'plugins/*/watch/*'],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // The window runs in a browser; its hooks follow the same rules as React's.
    files: ['src/**/*.{ts,tsx}', 'plugins/**/*.{ts,js}', 'tauri/src/*.js'],
    plugins: { 'react-hooks': reactHooks },
    languageOptions: { globals: { ...globals.browser } },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
    },
  },
  {
    // The command line tool, the build scripts and the tests run in Node.
    files: ['node/**/*.js', 'cli/**/*.ts', 'scripts/**/*.mjs', '**/*.test.ts', '*.config.{js,mts}'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // These are CommonJS, loaded with require.
    files: ['node/**/*.js', '*.config.js'],
    rules: { '@typescript-eslint/no-require-imports': 'off' },
  },
  {
    rules: {
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }],
      // Plugins declare the QuickJS host loosely, and a few errors are rewritten on purpose without their cause.
      '@typescript-eslint/no-explicit-any': 'off',
      'preserve-caught-error': 'off',
    },
  },
]);
