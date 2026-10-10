# Frontend mock data

This directory is intentionally outside `src/` and `tests/`. It contains stable,
UI-only fixtures for visual review and local prototype work with `pnpm dev:mock`.

## Hard invariant: mocks never ship

`vite.config.ts` aliases `@tauri-apps/api/core` to `tauri-core.ts` **only for the dev
server** in mock mode, and it throws if mock mode is requested for any other command
(`build`, `preview`). `tauri-core.ts` additionally throws when it is evaluated in a
production bundle. Both barriers exist because a build that silently carried the mock
IPC would present fabricated case data as evidence.

- `pnpm dev:mock` / `VITE_MOCK=true pnpm dev` — mock IPC active in the browser.
- `pnpm build`, `pnpm build --mode mock`, `VITE_MOCK=true pnpm build` — the last two fail
  with an explicit error; the first ships the real Tauri IPC.

Fixtures:

- `case-home.json` — active case, evidence sources, metrics, recent cases and jobs used by the VS Code-style shell.
- `import-dialog.json` — platform, source-mode and local-disk options used by the mirror import flow.

Keep secrets, real evidence paths and workstation-specific values out of this directory. Use synthetic paths and identifiers only.
