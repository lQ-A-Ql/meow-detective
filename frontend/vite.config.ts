import { defineConfig, loadEnv } from 'vite'
import path from 'path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'

const REACT_PACKAGES = new Set([
  'react',
  'react-dom',
  'scheduler',
  'react-router',
  'react-router-dom',
])

const STYLE_EXTENSIONS = ['.css', '.scss', '.sass', '.less', '.styl']

/**
 * Resolves the installed package name owning a module id, or undefined for app
 * source, virtual modules and stylesheets. Uses the last `node_modules/` segment
 * so pnpm's nested `.pnpm/<pkg>@<version>/node_modules/<pkg>` layout resolves to
 * the real package rather than the store directory.
 */
function packageNameFromModuleId(id: string): string | undefined {
  const normalized = id.replace(/\\/g, '/').split('?')[0]
  if (STYLE_EXTENSIONS.some((extension) => normalized.endsWith(extension))) {
    return undefined
  }
  const marker = '/node_modules/'
  const markerIndex = normalized.lastIndexOf(marker)
  if (markerIndex === -1) return undefined
  const segments = normalized.slice(markerIndex + marker.length).split('/')
  const [first, second] = segments
  if (!first) return undefined
  if (first.startsWith('@')) return second ? `${first}/${second}` : undefined
  return first
}

function vendorChunkFor(packageName: string): string {
  if (REACT_PACKAGES.has(packageName)) return 'vendor-react'
  if (packageName.startsWith('@tanstack/')) return 'vendor-query'
  if (packageName.startsWith('@radix-ui/')) return 'vendor-radix'
  if (packageName.startsWith('@fortawesome/')) return 'vendor-icons'
  if (
    packageName === 'i18next' ||
    packageName === 'react-i18next' ||
    packageName.startsWith('i18next-')
  ) {
    return 'vendor-i18n'
  }
  return 'vendor'
}

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
          ? {
              '@tauri-apps/api/core': path.resolve(__dirname, './mocks/tauri-core.ts'),
              '@tauri-apps/api/event': path.resolve(__dirname, './mocks/tauri-event.ts'),
            }
          : {}),
      },
    },
    // File types to support raw imports. Never add .css, .tsx, or .ts files to this.
    assetsInclude: ['**/*.svg', '**/*.csv'],
    build: {
      rollupOptions: {
        output: {
          /**
           * Keep the monolithic entry chunk from carrying every third-party
           * library. Only `node_modules` ids are grouped (app source returns
           * undefined and keeps Rollup's route-level splitting); stylesheets are
           * excluded so CSS emission is unchanged.
           */
          manualChunks(id) {
            const packageName = packageNameFromModuleId(id)
            if (!packageName) return undefined
            return vendorChunkFor(packageName)
          },
        },
      },
    },
  }
})
