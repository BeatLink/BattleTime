const { defineConfig } = require('eslint/config');
const expoConfig = require('eslint-config-expo/flat');

module.exports = defineConfig([
  expoConfig,
  {
    ignores: ['dist/*', 'node_modules/*', '.expo/*', 'android/*', 'ios/*', '.claude/*'],
  },
  {
    // The desktop shell and the build scripts run in Node, not in the bundle.
    files: ['electron/**/*.js', 'scripts/**/*.mjs'],
    languageOptions: {
      globals: { Buffer: 'readonly', __dirname: 'readonly', process: 'readonly' },
    },
  },
]);
