/**
 * Runtime build-mode helpers.
 *
 * Vite exposes `import.meta.env.DEV` and `import.meta.env.PROD` at build time.
 * - `pnpm dev` / `cargo tauri dev` → DEV = true
 * - `pnpm build` / `cargo tauri build` → PROD = true
 *
 * The V2 governance page is developer/audit-facing and should not appear in
 * normal production bundles.
 */
export function isDevOrAuditMode(): boolean {
  // DEV builds always show governance.
  if (import.meta.env.DEV) return true;
  // Audit/production builds can opt-in by setting VITE_AUDIT=true.
  if (import.meta.env.VITE_AUDIT === 'true') return true;
  return false;
}
