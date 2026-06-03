# AGENTS.md

## Project Overview

**Forensics Workbench** — a Tauri 2 desktop application for disk image forensic analysis on Windows. Rust backend handles evidence processing (disk images, file systems, Windows artifacts, search indexing, timeline generation). React/TypeScript frontend provides the investigator UI.

Single-user, desktop-first, Windows-primary. No HTTP server — all frontend↔backend communication goes through Tauri commands and events.

## Commands

```bash
# Backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p forensics-desktop          # build the Tauri shell crate

# Frontend (from frontend/)
pnpm install
pnpm dev                                  # Vite dev server (mock mode)
pnpm build                                # production build → frontend/dist

# Full desktop app (from apps/desktop/src-tauri/)
cargo tauri dev                           # launches Tauri with hot-reload frontend
cargo tauri build                         # release bundle
```

Rust toolchain: **stable** (pinned in `rust-toolchain.toml`, includes `rustfmt` + `clippy`).

Frontend package manager: **pnpm** (see `pnpm` overrides in `frontend/package.json`).

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  React UI (frontend/)                                   │
│  Vite + Tailwind 4 + React 18 + React Router 7          │
│  State: Zustand stores + TanStack Query                 │
├─────────────────────────────────────────────────────────┤
│  Tauri Command Layer (apps/desktop/src-tauri/commands/)  │
│  Thin wrappers: validate → call service → return DTO    │
├─────────────────────────────────────────────────────────┤
│  Application Services (crates/app-services/)            │
│  Orchestration logic per domain                         │
├─────────────────────────────────────────────────────────┤
│  Core Crates                                            │
│  domain / evidence-core / fs-* / image-* / search /     │
│  timeline / artifacts-windows / catalog / reports /      │
│  persistence-sqlite / infrastructure                    │
└─────────────────────────────────────────────────────────┘
```

### Data Flow (request path)

1. Frontend hook calls `apiClient.request(commandName, mockFallback, payload)`
2. In **tauri mode**: invokes Rust command via `@tauri-apps/api/core` → `invoke`
3. In **mock mode** (`VITE_API_MODE !== 'tauri'`): calls mock provider directly
4. Tauri command (annotated `#[tauri::command]`) delegates to `app-services`
5. Service returns a DTO from `crates/transport/src/dto/`

### Event Flow (push path)

Backend → Frontend via Tauri `emit`. Topics defined as constants in `crates/transport/src/events/mod.rs`. Frontend subscribes through `EventBus` (`src/lib/events/bus.ts`).

## Workspace Structure

| Crate | Role |
|-------|------|
| `domain` | Core entities defined: CaseId/CaseMeta/CaseSession, DataSource, FileEntry, Artifact, TimelineEvent, Job, Report, Tag |
| `app-services` | Application-layer orchestration per domain entity |
| `transport` | Shared DTOs, commands, events, errors, paging — the contract between frontend and backend |
| `persistence-sqlite` | SQLite repositories and migrations (9 repos, 18 migration scripts) |
| `evidence-core` | Disk image probing, volume detection, filesystem abstraction, reader |
| `fs-ntfs` / `fs-fat` / `fs-exfat` | Filesystem-specific parsers |
| `image-raw` / `image-e01` | Raw and E01 image format readers |
| `search` | Full-text indexing (tantivy), query parsing, highlighting |
| `timeline` | Timeline event generation and aggregation |
| `artifacts-windows` | Windows artifact parsers: EVTX, Prefetch, LNK, JumpList, Registry, RecycleBin, SRU, Thumbcache |
| `catalog` | File catalog indexing with ExtensionProjection, PathPrefixProjection, CatalogIndex |
| `reports` | Report generation: HTML, CSV, JSON, evidence bundle |
| `ingest` | Ingestion pipeline orchestration — IngestPipeline trait, IngestConfig, IngestSink, IngestStats |
| `mcp-client` | MCP (Model Context Protocol) client — SSE and Stdio transports |
| `runtime-cache` | Handle-based runtime caching |
| `infrastructure` | Cross-cutting: logging, hashing, filesystem utils, text, clock, config |
| `testing` | Test builders and fixtures |

## Conventions

### Rust

- All DTOs live in `crates/transport/src/dto/` — split into per-domain files (case.rs, files.rs, search.rs, etc.), re-exported from mod.rs. Never define serializable API types in other crates.
- DTOs use `#[serde(rename_all = "camelCase")]` — frontend receives camelCase JSON.
- `app-services` depends on both `domain` (entity types) and `transport` (DTO conversion).
- Optional fields use `#[serde(skip_serializing_if = "Option::is_none")]`.
- Tauri commands return `Result<T, String>` (Tauri 2 convention for serializable errors).
- Error type for cross-crate use: `transport::errors::ApiErrorDto`.
- Workspace dependencies are centralized in root `Cargo.toml` `[workspace.dependencies]`.
- Edition 2021 across all crates.

### Frontend

- Path alias: `@/` → `frontend/src/` (configured in `vite.config.ts` and `tsconfig.json`).
- `tsconfig.json` enables `strict: true` and path alias for IDE support.
- UI components: `src/app/components/ui/` (shadcn/radix-based primitives).
- Feature hooks: `src/features/<domain>/hooks.ts` — each wraps TanStack Query with `useQuery`/`useMutation`.
- API layer: `src/lib/api/<domain>.ts` — thin functions calling `apiClient.request(...)`.
- Mock data: `src/lib/api/mock-data.ts` — used when `VITE_API_MODE` is not `'tauri'`.
- Global state: Zustand stores in `src/stores/`.
- Styling: Tailwind CSS 4 (via `@tailwindcss/vite` plugin), custom theme in `src/styles/theme.css`.
- Router: React Router 7, flat route definitions in `src/app/routes.tsx`.

### Naming

- Rust crates: kebab-case (`artifacts-windows`, `app-services`).
- Rust modules: snake_case (`case_service.rs`, `file_commands.rs`).
- Frontend files: PascalCase for components (`FileBrowser.tsx`), camelCase for hooks/utils (`hooks.ts`).
- DTO suffix: Rust types end in `Dto` (`CaseSummaryDto`); frontend interfaces drop the suffix (`CaseSummary`) except `TimelineEventDto` which kept it.

### Layout Components

All app-shell layout components live in `src/components/layout/`: AppShell, Layout, TopBar, BottomDrawer, InspectorPane, PageSubbar. UI primitives (shadcn) stay in `src/app/components/ui/`. Do not create new layout components under `src/app/components/`.

## Gotchas

1. **Mock vs Tauri mode**: The frontend runs standalone with mock data by default (`pnpm dev`). Set `VITE_API_MODE=tauri` to hit real Rust commands. The `ApiClient` class switches behavior based on this env var at build time.

2. **Frontend dist path**: Tauri expects the built frontend at `frontend/dist` (relative from `src-tauri/`). The path is hardcoded in `tauri.conf.json` as `"../../../frontend/dist"`.

3. **Transport crate is the contract**: DTOs are in per-domain files under `crates/transport/src/dto/` (case.rs, files.rs, search.rs, timeline.rs, artifacts.rs, jobs.rs, viewer.rs, reports.rs). Any change must happen here first. Both the Tauri command layer and the frontend `types/models.ts` must stay in sync manually — there is no codegen yet.

4. **domain crate is implemented**: Core types (CaseMeta, FileEntry, Artifact, TimelineEvent, Job, Report, Tag, DataSource) are defined with serde support. Most crates are fully implemented: `persistence-sqlite` (9 repos, 18 migrations), `evidence-core` (image probing, volume detection), `fs-ntfs`/`fs-fat`/`fs-exfat` (filesystem parsers), `artifacts-windows` (9 extractors), `search` (tantivy indexing), `catalog` (ExtensionProjection, PathPrefixProjection), `ingest` (pipeline trait), `mcp-client` (SSE + Stdio transports).

5. **Frontend test framework**: Vitest is configured with jsdom environment. Run `pnpm test` from `frontend/`. 24 test files, 81 tests covering pages (Settings, DataAnalysis, FileBrowser, Search, Timeline, Artifacts), viewers (Hex, Text, Image), stores (ui-store, selection-store), API layer, and hooks. Coverage thresholds: 45% branches, 35% functions/lines/statements.

6. **Tailwind 4 with `source(none)`**: The Tailwind config uses `@import 'tailwindcss' source(none)` with explicit `@source` directive. Don't add a `tailwind.config.js` — configuration is CSS-first.

7. **Event topics are string constants**: Defined in `crates/transport/src/events/mod.rs` and mirrored as a TypeScript union type `EventTopic` in `src/types/models.ts`. Keep them in sync.

8. **Tauri 2**: This project uses Tauri v2 (not v1). Commands use `#[tauri::command]` with the v2 handler registration pattern. The `Emitter` trait is used for events.

## Key Design Documents

- `PRD.md` — Product requirements
- `spec.md` — Technical specification
- `design.md` — Detailed architecture, crate responsibilities, data structures, MVP phases
- `ci.md` — CI pipeline design (GitHub Actions structure, check steps, caching rules)
- `test-plan.md` — Testing strategy
- `autopsy-borrowings.md` — Concepts borrowed from Autopsy (reference forensic tool)

## Adding a New Feature (typical flow)

1. Define/extend DTOs in `crates/transport/src/dto/<domain>.rs` (then re-export in mod.rs)
2. Add command request types in `crates/transport/src/commands/mod.rs` if needed
3. Implement service logic in `crates/app-services/src/<domain>_service.rs`
4. Wire Tauri command in `apps/desktop/src-tauri/src/commands/<domain>_commands.rs`
5. Register command in `apps/desktop/src-tauri/src/lib.rs` `invoke_handler`
6. Mirror DTO as TypeScript interface in `frontend/src/types/models.ts`
7. Add API function in `frontend/src/lib/api/<domain>.ts`
8. Add mock data in `frontend/src/lib/api/mock-data.ts`
9. Create/update hook in `frontend/src/features/<domain>/hooks.ts`
10. Build page/component consuming the hook
