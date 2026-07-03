/// <reference types="vite/client" />

interface ImportMetaEnv {
  // Intentionally empty: runtime API feature flags are not exposed here.
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
