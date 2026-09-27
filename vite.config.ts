import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  optimizeDeps: { include: ['pdf-lib', '@pdf-lib/fontkit', 'pdfjs-dist', 'tesseract.js'] },
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_'],
  build: { target: 'es2022' },
  worker: { format: 'es' },
  test: { include: ['src/**/*.test.ts', 'tests/**/*.test.ts'] },
});
