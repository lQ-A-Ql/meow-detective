import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'node:path';

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { '@': path.resolve(__dirname, 'src') } },
  test: {
    globals: true,
    environment: 'jsdom',
    environmentOptions: { jsdom: { pretendToBeVisual: true } },
    setupFiles: ['tests/setup-file-browser-real.ts'],
    include: ['tests/features/files/file-browser-linkage.real.test.tsx'],
    testTimeout: 15000,
  },
});
