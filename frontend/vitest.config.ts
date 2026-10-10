import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./tests/setup.ts'],
    include: ['tests/**/*.{test,spec}.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json-summary', 'lcov'],
      reportsDirectory: 'coverage',
      // Without an explicit include list only modules a test actually imported are
      // reported, so an entirely untested module was simply absent from the report and
      // could never fail the gate. Include the whole source tree and keep the thresholds
      // below what the suite genuinely reaches today.
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/**/*.d.ts',
        'src/main.tsx',
        'src/vite-env.d.ts',
        'src/i18n/**',
        'src/app/components/ui/**',
      ],
      // Real values with the full source tree included are roughly
      // lines 72 / statements 69 / functions 64 / branches 62; the gate sits just below so a
      // regression fails while ordinary refactoring does not.
      thresholds: {
        lines: 70,
        statements: 67,
        functions: 62,
        branches: 60,
      },
    },
  },
});
