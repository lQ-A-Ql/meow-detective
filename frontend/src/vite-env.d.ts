/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_MODE?: 'mock' | 'tauri';
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
