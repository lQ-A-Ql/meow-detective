import { defineConfig, loadEnv } from 'vite'
import path from 'path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'

/**
 * Mock mode exists only so the workbench can be reviewed in a browser without the
 * Tauri host (`pnpm dev:mock`). It must never reach a build: a bundle carrying the
 * mock IPC would present fabricated case data as if it were evidence. Two
 * independent barriers enforce that:
 *   1. this config refuses to run any non-serve command (build/preview) when mock
 *      mode is requested, and
 *   2. mocks/tauri-core.ts throws if it is ever evaluated in a production bundle.
 */
export default defineConfig(({ mode, command }) => {
  const env = loadEnv(mode, process.cwd(), 'VITE_')
  const mockRequested = mode === 'mock' || env.VITE_MOCK === 'true'

  if (mockRequested && command !== 'serve') {
    throw new Error(
      `Mock mode is dev-server only (pnpm dev:mock); refusing to run "${command}" so mock fixtures cannot be bundled into a build.`,
    )
  }

  const mockEnabled = mockRequested && command === 'serve'

  return {
    plugins: [
      // The React and Tailwind plugins are both required for Make, even if
      // Tailwind is not being actively used – do not remove them
      react(),
      tailwindcss(),
    ],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
        ...(mockEnabled
          ? { '@tauri-apps/api/core': path.resolve(__dirname, './mocks/tauri-core.ts') }
          : {}),
      },
    },
    // File types to support raw imports. Never add .css, .tsx, or .ts files to this.
    assetsInclude: ['**/*.svg', '**/*.csv'],
  }
})
