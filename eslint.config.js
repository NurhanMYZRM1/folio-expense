// ESLint reads the web/TypeScript code and flags likely bugs before they ship.
// Run it with `npm run lint`. The Rust side is linted separately by Clippy
// (`npm run lint:rust`).
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import globals from 'globals';

export default tseslint.config(
  // Generated, vendored, or build-output files are never linted.
  {
    ignores: [
      'dist',
      'public',
      'src-tauri',
      'src/bindings/generated',
      'test-results',
      'playwright-report',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat['recommended-latest'],
  {
    languageOptions: {
      ecmaVersion: 2022,
      globals: { ...globals.browser, ...globals.worker },
    },
    rules: {
      // The screens reset and reload their data in an effect when the selected
      // record changes. That works correctly, so this performance advice is a
      // warning (shown, but not a CI failure) until those screens are refactored.
      'react-hooks/set-state-in-effect': 'warn',
    },
  },
  // Build scripts, configs and tests run in Node, not the browser.
  {
    files: ['scripts/**', 'tests/**', '*.config.{js,ts}'],
    languageOptions: { globals: globals.node },
  },
);
