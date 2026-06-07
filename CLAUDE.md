# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Forensics Workbench is a Windows-first, single-user desktop digital forensics application. The shipped app is a Tauri 2 desktop shell with a Rust backend, a React/TypeScript frontend, and SQLite-backed case storage. The first-version product scope is local/offline disk-media investigation: cases, evidence import, file browsing, search, timeline analysis, Windows artifacts, and report export.

The intended architecture is backend-led: Rust owns case lifecycle, data source import, evidence readers, search/timeline/artifact/report logic, and persistence. React primarily consumes Tauri commands, DTO snapshots, and events.

## Common commands

Run commands from the repository root unless noted.

### Rust backend

- Format check: `cargo fmt --all -- --check`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Test all Rust crates: `cargo test --workspace`
- Test one crate: `cargo test -p <crate-name>` (for example `cargo test -p app-services`)
- Test one Rust test by name: `cargo test -p <crate-name> <test_name>`
- Build/check the Tauri Rust crate: `cargo check -p forensics-desktop`

### Frontend

The frontend package lives in `frontend/` and uses pnpm.

- Install dependencies: `pnpm --dir frontend install --frozen-lockfile`
- Dev server: `pnpm --dir frontend dev`
- Build frontend: `pnpm --dir frontend build`
- Typecheck: `pnpm --dir frontend typecheck`
- Lint: `pnpm --dir frontend lint`
- Test all frontend tests: `pnpm --dir frontend test`
- Watch frontend tests: `pnpm --dir frontend test:watch`
- Run one frontend test file: `pnpm --dir frontend vitest run src/path/to/file.test.tsx`
- Coverage: `pnpm --dir frontend test:coverage`

### Desktop app

- Tauri config is at `apps/desktop/src-tauri/tauri.conf.json`; it points `frontendDist` to `frontend/dist`.
- For a production desktop build path, build the frontend first, then run/check the Tauri crate as needed.

### Repository guard scripts

These PowerShell scripts encode important architectural/security boundaries:

- Coverage harness: `powershell -File scripts/run-coverage.ps1` (`-Rust`, `-Frontend`, and `-StrictRustTool` are supported)
- Ensure Tauri command layer does not contain raw SQL: `powershell -File scripts/check-command-sql-boundary.ps1`
- Ensure media preview stays on the guarded `evidence-media:` protocol path: `powershell -File scripts/check-media-protocol-guard.ps1`
- Release/debug-string guard: `powershell -File scripts/check-release-guard.ps1`
- Additional targeted guards exist in `scripts/` for deny exceptions, EVTX dependency decisions, import optimization, E01 profiling/performance, fixture generation, and WebView2 media smoke checks.

## Architecture map

### Workspace layout

- `apps/desktop/src-tauri/` — Tauri host application. `src/lib.rs` registers all commands, the media protocol, the dialog plugin, and shared `AppState`.
- `frontend/` — React/Vite/TypeScript UI. Route-level pages are under `frontend/src/app/pages/`; reusable layout/viewer/tree/table components are under `frontend/src/components/`; API wrappers live under `frontend/src/lib/api/`; React Query hooks live under `frontend/src/features/`; Zustand stores live under `frontend/src/stores/`.
- `crates/domain/` — core domain entities and IDs such as cases, data sources, file entries, artifacts, jobs, reports, tags, timestamps, and timeline events.
- `crates/app-services/` — use-case orchestration for cases, files, imports, jobs, search, timeline, reports, text, analysis, staging, streaming, and performance.
- `crates/persistence-sqlite/` — SQLite connection/migration/repository layer. Keep SQL here or in lower repository/service layers, not in Tauri command handlers.
- `crates/transport/` — command DTOs, event DTOs, paging, and error shapes shared across the IPC boundary.
- `crates/evidence-core/`, `crates/image-raw/`, `crates/image-e01/`, `crates/fs-ntfs/`, `crates/fs-fat/`, `crates/fs-exfat/` — read-only evidence/image/filesystem abstractions and implementations.
- `crates/artifacts-core/`, `crates/artifacts-windows/` — artifact extraction framework and Windows artifact parsers.
- `crates/search/`, `crates/timeline/`, `crates/catalog/`, `crates/reports/` — feature services for indexing/querying, event projection, catalog management, and report generation.
- `crates/runtime-cache/` — temporary runtime cache support; it must not become the source of truth.
- `crates/mcp-client/` — MCP client integration exposed through desktop commands and settings UI.
- `crates/testing/` and `testdata/` — shared testing utilities and fixtures.

### Backend layering

The main dependency direction is domain/transport at the core, app-services for use cases, persistence/infrastructure for storage and external concerns, then the Tauri command layer as a thin IPC adapter. Future changes should preserve that direction: command handlers should validate/translate request DTOs and delegate, not implement business workflows or SQL directly.

`AppState` in `apps/desktop/src-tauri/src/state/app_state.rs` holds the active case, task manager, SQLite pool, MCP clients/config, and settings paths. Case opening initializes the database pool with WAL, foreign keys, busy timeout, and normal synchronous mode; case closing should release it.

Long-running operations are represented as jobs/tasks and surfaced through job snapshots/events. Import flows should invalidate or refresh frontend case/files/timeline/artifacts/search queries because they populate multiple projections.

### Frontend architecture

The frontend uses React Router (`frontend/src/app/routes.tsx`) with lazy page modules for Case Home, Data Analysis, Files, Search, Timeline, Artifacts, Reports, and Settings. React Query is the default server-state layer (`frontend/src/app/providers.tsx`) with a 30 second stale time and no refetch on window focus. Zustand stores hold local UI/selection/MCP state.

API modules in `frontend/src/lib/api/` call a small client wrapper that chooses live Tauri IPC when `VITE_API_MODE=tauri` or `isTauri()` is true, otherwise mock data. Feature hooks in `frontend/src/features/*/hooks.ts` wrap API calls and cache invalidation; prefer adding new calls there rather than invoking Tauri directly from page components.

### Evidence preview/media constraints

Evidence content must be read through the evidence reader path keyed by `FileEntryId` and data source type. Do not build host filesystem paths from case roots and file entry paths to preview evidence.

Media preview has a guarded split path: small content may be returned as bounded data URLs; larger audio/video uses the custom `evidence-media://handle/<encoded>` protocol registered by Tauri. The CSP in `tauri.conf.json` allows `media-src 'self' data: evidence-media:`. Keep `scripts/check-media-protocol-guard.ps1` passing when touching file preview, media protocol, viewer DTOs, or frontend media hooks.

## Tests and fixtures

- Frontend Vitest config includes `src/**/*.{test,spec}.{ts,tsx}` with jsdom and coverage thresholds in `frontend/vitest.config.ts`.
- Tiny CI fixtures are documented in `docs/architecture-model.md`, including logical, RAW, synthetic E01, synthetic registry hive, and EVTX samples under `testdata/fixtures/tiny/`.
- `scripts/run-coverage.ps1` uses `cargo llvm-cov` for Rust if installed and Vitest coverage for frontend.

## Documentation worth checking

- `PRD.md` — product goals and first-version scope.
- `spec.md` — technical architecture principles and service responsibilities.
- `docs/architecture-model.md` — current architecture model, module responsibilities, data flow, and implementation constraints.
- `ci.md` — intended CI gates and command sequence.
- `test-plan.md` — test layering, naming, and fixture strategy.
