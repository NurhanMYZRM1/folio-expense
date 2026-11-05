import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import globals from 'globals';

// Use the root's installed lint toolchain, scoped to mobile sources.
export default tseslint.config(
  { ignores: ['node_modules/**', '.expo/**', 'ios/**', 'android/**', 'modules/**/android/**', 'dist/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat['recommended-latest'],
  {
    files: ['**/*.{ts,tsx,js,cjs,mjs}'],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: { 'react-hooks/set-state-in-effect': 'warn', '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }] },
  },
  {
    files: ['src/native/motion.tsx'],
    // Reanimated shared values intentionally mutate through the native API.
    rules: { 'react-hooks/immutability': 'off' },
  },
);
