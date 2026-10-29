import desktopConfig from '../eslint.config.js';

export default [
  ...desktopConfig.map((config) =>
    config.ignores
      ? { ...config, ignores: config.ignores.filter((path) => path !== 'mobile') }
      : config,
  ),
  { rules: { '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }] } },
  // Reanimated shared values are mutable animation handles, not React state.
  { files: ['src/native/motion.tsx'], rules: { 'react-hooks/immutability': 'off' } },
  { ignores: ['ios/**', 'android/**', 'modules/**', '.expo/**'] },
];
