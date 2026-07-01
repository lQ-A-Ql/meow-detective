# Forensics Workbench — Comprehensive Audit Report

**Date:** 2026-06-30  
**Project:** Forensics Workbench (Tauri 2 + Rust workspace + React 18 frontend)  
**Auditor:** MiMoCode Compose Agent  
**Scope:** Backend code quality, frontend code quality, architecture / data flow, and deep dives into search/catalog indexing, timeline generation, and forensic overview (backend artifact extraction + frontend overview pages).  
**Deliverable:** This Markdown report, plus supporting section files under `docs/compose/reports/`.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Scope and Methodology](#2-scope-and-methodology)
3. [Quality Gates Summary](#3-quality-gates-summary)
4. [Backend Code Quality Analysis](#4-backend-code-quality-analysis)
5. [Frontend Code Quality Analysis](#5-frontend-code-quality-analysis)
6. [Architecture and Data Flow Analysis](#6-architecture-and-data-flow-analysis)
7. [Deep Dive — Search and Catalog Indexing](#7-deep-dive--search-and-catalog-indexing)
8. [Deep Dive — Timeline Generation](#8-deep-dive--timeline-generation)
9. [Deep Dive — Forensic Overview](#9-deep-dive--forensic-overview)
10. [Consolidated Risk Register and Recommendations](#10-consolidated-risk-register-and-recommendations)
11. [Appendix: Audit Evidence](#11-appendix-audit-evidence)

---

## 1. Executive Summary

Forensics Workbench is a mature, backend-led Tauri desktop forensic application with strong architectural discipline and a clear separation between Rust evidence processing and the React investigator UI. This audit covered the entire stack: 38 Rust crates, a 17-page routed React frontend (23 .tsx files including settings sub-pages), the Tauri IPC layer, and the three focus areas requested by the project owner.

**Overall posture:**

- **Architecture and layering are excellent.** Dependency direction is strictly inward; parser crates have no Tauri or frontend dependencies; Tauri commands are thin IPC adapters.
- **Type safety and error taxonomy are strong.** Most crates use `thiserror` enums and the shared `ApiErrorDto` contract. A handful of residual `Result<String, String>` functions remain.
- **Frontend code quality is solid** with centralized API routing, TanStack Query for server state, Zustand for local state, and coverage above the configured thresholds.
- **Forensic capabilities are Windows-primary.** Windows artifact extraction (registry, EVTX, browser, prefetch, LNK, Recycle Bin, SRU, thumbcache) is mature; Linux, macOS, and mobile crates exist but are not wired into the main extraction pipeline.
- **Search and timeline are well-engineered** but have UX/visibility gaps: indexing is coupled to artifact analysis, the catalog crate is dead code, timeline zoom controls are not wired, and `source_object_id` correlation is not enforced.

**Most important risks before V2 release:**

1. Manual Rust/TypeScript DTO synchronization is the largest long-term drift risk.
2. Non-Windows artifact crates are not integrated into `analysis_service::extraction`.
3. Six residual `Result<String, String>` functions and 68 production `unwrap()` calls remain.
4. Five production modules exceed the 1500-line policy limit.
5. The frontend search placeholder advertises SQL-like syntax the backend does not support.
6. Timeline zoom/granularity controls are UI-only and do not affect query behavior.

**Quick gates passed during this audit:** `cargo fmt --all -- --check`, `pnpm --dir frontend typecheck`, `pnpm --dir frontend lint`, and all seven relevant PowerShell guard scripts. Full `cargo clippy` and `cargo test` were not re-run because they require a Visual Studio developer environment.

---

## 2. Scope and Methodology

### Audit scope

| Area | Coverage |
|---|---|
| Backend code quality | Rust workspace: typed errors, module size, dead code, unsafe usage, `unwrap`/`expect` density, dependency centralization, SQL boundary |
| Frontend code quality | React/TypeScript: component/page sizes, API layer discipline, state management, test coverage, lint/typecheck status |
| Architecture and data flow | Crate layering, Tauri IPC contract, DTO/event mirroring, security boundaries (media protocol, path validation, MCP) |
| Search and catalog indexing | Tantivy full-text index, ingest-time indexing, catalog projections, frontend Search page |
| Timeline generation | MACB projection, artifact-derived events, aggregation, frontend Timeline page, `source_object_id` correlation |
| Forensic overview | Artifact extraction pipeline, `artifacts-*` crate coverage, frontend CaseOverview / V3Dashboard / V3ScoreCards / Artifacts pages |

### Methodology

1. **Project metadata collection** — verified crate counts, command counts, repository counts, migrations, and governance files.
2. **Quick quality gates** — ran `cargo fmt --all -- --check`, `pnpm --dir frontend typecheck`, `pnpm --dir frontend lint`, and relevant PowerShell guard scripts.
3. **Backend static analysis** — scanned for `Result<String, String>`, `#[allow(dead_code)]`, `unsafe` blocks, `unwrap`/`expect` density, module sizes, and direct version dependencies.
4. **Frontend static analysis** — inspected page sizes, API-layer discipline, store counts, test coverage via `pnpm test:coverage`, and TanStack Query usage.
5. **Architecture review** — traced the request/event paths, inspected `Cargo.toml` dependency direction, and reviewed security boundaries.
6. **Focused deep dives** — for each focus area, traced the production code paths from frontend → command → service → parser → persistence, then documented strengths, risks, and recommendations.

### What this audit did not do

- Did not run `cargo clippy --workspace --all-targets -- -D warnings` or `cargo test --workspace` (require VS linker environment).
- Did not perform dynamic analysis or penetration testing.
- Did not modify production code.

---

## 3. Quality Gates Summary

All quick gates that could run in the current environment passed on the audit date.

| Check | Command | Result |
|---|---|---|
| Rust formatting | `cargo fmt --all -- --check` | PASS (zero diff) |
| Frontend typecheck | `pnpm --dir frontend typecheck` | PASS |
| Frontend lint | `pnpm --dir frontend lint` | PASS |
| Command/SQL boundary | `scripts/check-command-sql-boundary.ps1` | PASS |
| Dead-code guard | `scripts/check-dead-code-allow-guard.ps1` | PASS |
| Media protocol guard | `scripts/check-media-protocol-guard.ps1` | PASS |
| Frontend lockfile policy | `scripts/check-frontend-lockfile-policy.ps1` | PASS |

The full desktop build gates (`cargo clippy --workspace --all-targets`, `cargo test --workspace`, `cargo deny check`, the remaining PowerShell guard scripts) were not re-run as part of this audit per the explicit scope constraint.

### Project metadata

- **Workspace crates:** 38 (37 library crates + Tauri shell at `apps/desktop/src-tauri`)
- **Tauri commands:** 96
- **SQLite repositories:** 15
- **SQL migrations:** 33 (`0001`–`0031` plus `staging_001.sql`)
- **Frontend pages:** 23 non-test `.tsx` files in `frontend/src/app/pages/` (17 top-level + 6 `settings/` sub-pages)
- **Frontend test files:** 76 `*.test.{ts,tsx}`
- **Release scorecard:** 81/100 Grade B (from `docs/release-scorecard.md`)
- **Known limitations:** 36 documented items in `testdata/governance/v2-known-limitations.json`

---

## 4. Backend Code Quality Analysis

- The backend is **strongly typed and well-layered**: the majority of crates use domain-specific `thiserror` enums, and Tauri commands delegate to `app-services` rather than mixing business logic or SQL. Six residual `Result<String, String>` functions remain in `exchange` and `artifacts-windows` registry code as exceptions.
- **The quick-gate subset run during this audit passed**: `cargo fmt`, `pnpm typecheck`, `pnpm lint`, command/SQL boundary, dead-code, media-protocol, and frontend lockfile guards all passed on the audit date. The full `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` gates were not re-run as part of this audit.
- A few **legacy untyped error surfaces** remain (`Result<String, String>` in 6 places), mostly in registry/Windows artifact parsers and the STIX serializer.
- **Module size discipline has slipped**: five production `.rs` files exceed the 1500-line project ceiling, with one file (`file_service/viewer.rs`) at >3,000 lines.
- **Unsafe usage is minimal and documented**: only four production `unsafe` blocks exist, and all carry `// SAFETY:` comments; no dead-code suppressions remain in production code.
- **Workspace dependency centralization has minor drift**: six direct version dependencies exist outside the workspace root, all in non-vendored crates; the vendored `evtx-patched` crate is intentionally exempt.
- **Unwrap/expect density is a latent runtime risk**: 68 production `.unwrap()` calls and 38 production `.expect()` calls remain outside `#[cfg(test)]` blocks, with the highest concentration in the vendored EVTX parser.

### Quality Metrics

| Metric | Count | Notes / Source |
|---|---|---|
| Workspace crates | 38 | 37 library crates + Tauri shell (`apps/desktop/src-tauri`) |
| Tauri commands | 96 | Registered in `apps/desktop/src-tauri/src/lib.rs` |
| SQLite repositories | 15 | `crates/persistence-sqlite/src/repositories/` |
| SQL migrations | 33 | `0001`–`0031` plus `staging_001.sql` |
| Production `.rs` files >1500 lines | 5 | See "Oversized Modules" below |
| `Result<String, String>` occurrences | 6 | Residual untyped error returns |
| `#[allow(dead_code)]` in production | 0 | All matches are in `#[cfg(test)]` blocks |
| Production `unsafe` blocks | 4 | All have `// SAFETY:` comments |
| `unsafe` blocks in tests | 3 | All have `// SAFETY:` comments |
| `cargo deny` advisory exceptions | 16 | All expire 2026-09-01; all tied to Tauri/gtk/urlpattern transitive crates |
| Command files inspected for raw SQL | 2 | `case_commands.rs`, `file_commands.rs` — no production raw SQL found |
| Direct version dependencies (non-vendored) | 6 | `app-services`, `image-e01`, `mcp-client`, `search`, `updater`, `desktop` shell — see drift section |
| Direct version dependencies in vendored `evtx-patched` | 15 | `ahash`, `bitflags`, `bumpalo`, `crc32fast`, `glob`, `goblin`, `jiff`, `log`, `rayon`, `serde`, `serde_json`, `sonic-rs`, `utf16-simd`, `winstructs`, `zmij` |
| Production `.unwrap()` occurrences (outside `#[cfg(test)]`) | 68 | See unwrap/expect density subsection |
| Production `.expect()` occurrences (outside `#[cfg(test)]`) | 38 | See unwrap/expect density subsection |

### Highlighted Strengths

1. **Typed error taxonomy in production**. `crates/transport/src/errors.rs` defines a single `ApiErrorDto` plus a `CommandError` with forensic-aware categories (`validation`, `parser`, `security`, `external`, `timeout`, etc.). Service crates such as `case_service.rs` use `thiserror` enums (`CaseServiceError`) and never return `Result<T, String>`.
2. **Clean command/service/SQL boundary**. `case_commands.rs` and `file_commands.rs` contain no production raw SQL strings; all persistence logic lives in repository modules (`case_repo.rs`, `file_repo.rs`). The only raw SQL in `commands/` is inside `#[cfg(test)]` benchmark fixtures in `apps/desktop/src-tauri/src/commands/benchmarks.rs:169,174`, which the SQL-boundary guard intentionally exempts. The guard script `scripts/check-command-sql-boundary.ps1` passed on production paths.
3. **Dead-code policy is enforced**. `#[allow(dead_code)]` appears only in test files (`app-services/tests/registry_fixture_expected_test.rs`).
4. **Unsafe usage is minimal and audited**. The only production `unsafe` blocks are in Windows Compression API calls (`artifacts-windows/src/prefetch/parser.rs`) and Windows process-memory accounting (`app-services/src/import_analysis/progress.rs`). Each block has a `// SAFETY:` comment explaining why the call is sound.
5. **Dependency governance is explicit**. `deny.toml` requires `owner`, `reason`, and `expires` for every exception and disallows unknown registries or git sources. All 16 advisory exceptions are documented and currently unexpired.

### Risks and Issues Found

#### 1. Residual untyped error returns (`Result<String, String>`)

Six occurrences remain in production code. These bypass the typed error taxonomy and make error classification harder in the UI.

- `crates/exchange/src/stix.rs:317`
- `crates/artifacts-windows/src/registry/txlog.rs:405`
- `crates/artifacts-windows/src/registry/parser.rs:62`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:3`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:11`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:23`

#### 2. Oversized production modules (>1500 lines)

The project ceiling is 1500 lines per production source file. Five files exceed it:

| File | Lines | Risk |
|---|---|---|
| `crates/app-services/src/file_service/viewer.rs` | 3,044 | Preview/E01 reader cache, file handle resolution, and range reading are all in one file; hard to unit-test in isolation. |
| `crates/fs-ntfs/src/lib.rs` | 2,026 | NTFS reader, data runs, MFT parsing, and path resolution are tightly coupled in one module. |
| `crates/app-services/src/analysis_service/extraction/email.rs` | 1,903 | EML/MBox/PST extraction logic is oversized; should be split into `eml.rs`, `mbox.rs`, `pst.rs`. |
| `crates/fs-apfs/src/lib.rs` | 1,523 | Filesystem parser exceeds the limit. |
| `crates/artifacts-windows/src/evtx/parser.rs` | 1,505 | EVTX parser is at the boundary but should be split by channel/event type. |

#### 3. Dependency advisory exception concentration

`deny.toml` carries 16 advisory exceptions, all expiring on `2026-09-01`. They are all transitive issues pulled in by Tauri (gtk3 bindings) or `urlpattern`/`tauri-utils`. If upstream has not released fixes by the expiry date, the project will either need to renew exceptions or absorb a breaking Tauri upgrade.

#### 4. Workspace dependency centralization drift

`AGENTS.md` mandates workspace-centralized dependencies. A scan of all member `Cargo.toml` files found 21 direct version dependencies outside the workspace `[workspace.dependencies]` table. The vendored `evtx-patched` crate accounts for 15 of these and is intentionally exempt because it is a patched fork that must remain close to upstream.

The remaining six direct versions are in non-vendored, project-owned crates and represent drift against the centralization policy:

| Crate | Direct dependency | Note |
|---|---|---|
| `app-services` | `unicode-normalization = "0.1"` | Used by filename/path normalization; should move to workspace. |
| `image-e01` | `flate2 = "1"` | E01 compression/decompression; should move to workspace. |
| `mcp-client` | `async-trait = "0.1"` | MCP transport traits; should move to workspace. |
| `search` | `tantivy = "0.26"` | Full-text index engine; should move to workspace. |
| `updater` | `tauri-plugin-updater = "2"` | Tauri plugin; version may be intentional for plugin compatibility. |
| `desktop` shell | `tauri-plugin-dialog = "2"` | Tauri plugin; version may be intentional for plugin compatibility. |

All other dependencies are either `{ workspace = true }` or path-only internal crates. None of the direct versions are in dev-only or build-only paths; they are all normal production dependencies.

#### 5. Unwrap/expect density in production code

After stripping `#[cfg(test)] mod` blocks, the production Rust source contains **68 `.unwrap()` calls** and **38 `.expect()` calls** across the workspace and Tauri command layer. The top files by occurrence are:

| Rank | File | `.unwrap()` count | Context |
|---|---|---|---|
| 1 | `crates/evtx-patched/src/evtx_parser.rs` | 21 | Vendored EVTX parser main entry point. |
| 2 | `crates/evtx-patched/src/evtx_chunk.rs` | 8 | Vendored EVTX chunk iteration. |
| 3 | `crates/fs-apfs/src/checkpoint.rs` | 7 | APFS checkpoint parsing. |

| Rank | File | `.expect()` count | Context |
|---|---|---|---|
| 1 | `crates/evtx-patched/src/wevt_templates/cache.rs` | 6 | Vendored WEVT template cache. |
| 2 | `crates/app-services/src/governance/fact_loader.rs` | 6 | Embedded governance JSON loading. |
| 3 | `crates/evtx-patched/src/binxml/ir_json.rs` | 4 | Vendored EVTX JSON renderer. |

This is a runtime-safety risk for a forensic tool that processes attacker-controlled evidence. A malformed EVTX chunk, APFS checkpoint, or corrupted governance bundle can currently trigger a panic rather than a typed error returned to the UI. The vendored `evtx-patched` crate is exempt from normal project quality gates, but it is still on the hot path for untrusted input. The other top files (`fact_loader.rs`, `checkpoint.rs`) are project-owned and should be migrated to `?` and typed errors.

#### 6. Dynamic SQL composition in repositories

Dynamic SQL is not limited to `file_repo.rs`. Several repositories build statements with `format!` for column lists, `IN (...)` placeholders, or conditional `WHERE` clauses:

- `file_repo.rs`: `format!("SELECT {FILE_ENTRY_COLUMNS} ...")` and dynamic `IN ({})` placeholders.
- `graph_repo.rs`: dynamic `IN` filter for edge types in `build_neighbor_query`, plus `format!` for column lists.
- `timeline_repo.rs`: `format!` for `WHERE 1=1` conditional clauses and LIMIT/OFFSET placeholders.
- `notebook_repo.rs`: `format!` for partial UPDATE `SET` clauses, LIKE filters, recursive CTE columns, and `IN` placeholders for batch citations.
- `batch_repo.rs`: `format!` for conditional `UPDATE ... SET status = ?1{now}` clauses.
- `audit_repo.rs`: `format!` for conditional `WHERE 1=1` case/action filters and LIMIT/OFFSET placeholders.

In every case the user-facing values are parameterized, so the current risk is low. However, the pattern is repeated across six repositories, and future changes could accidentally concatenate user input into a `format!` string. The project should centralize a single helper for `IN` placeholders, conditional `WHERE` clauses, and column-list expansion.

#### 7. Error classification relies on substring matching

`CommandError::from_service_error` in `crates/transport/src/errors.rs` maps service errors to categories by scanning the lower-cased message for substrings such as `"timeout"`, `"parse"`, `"not supported"`, etc. This is brittle and can misclassify new error variants that happen to contain those words.

### Improvement Recommendations

#### P0 — Remediate before next release

1. **Convert remaining `Result<String, String>` functions to typed errors.** Create small error enums in `exchange` and `artifacts-windows` (e.g., `RegistryError`, `StixError`) and return those instead. This completes the typed-error migration started in the V5 audit.
2. **Split `crates/app-services/src/file_service/viewer.rs`.** Decompose it into at least:
   - `viewer/handle.rs` — file handle creation and cache-key logic
   - `viewer/range.rs` — range reads and byte streaming
   - `viewer/e01_cache.rs` — per-case E01 reader cache
   - `viewer/preview.rs` — preview descriptor logic
3. **Split `crates/fs-ntfs/src/lib.rs`.** Separate `mft.rs`, `data_runs.rs`, `attribute.rs`, and `reader.rs` to make the NTFS parser maintainable and testable.

#### P1 — Near-term engineering debt

4. **Split the remaining oversized files**: `email.rs`, `fs-apfs/src/lib.rs`, and `evtx/parser.rs`.
5. **Refresh dependency exceptions before 2026-09-01.** Evaluate whether Tauri 2.x or `urlpattern` updates are available; if not, extend the exceptions with a fresh technical review and updated expiry dates.
6. **Centralize the six non-vendored direct version dependencies in the workspace root.** Move `unicode-normalization`, `flate2`, `async-trait`, `tantivy`, `tauri-plugin-updater`, and `tauri-plugin-dialog` to the root `[workspace.dependencies]` table and reference them with `{ workspace = true }`. Keep `evtx-patched` as the documented exemption.
7. **Reduce unwrap/expect density in project-owned code.** Prioritize `fact_loader.rs` and `fs-apfs/src/checkpoint.rs`, then audit the remaining project-owned files. Replace each call with `?` and a typed error where the failure is recoverable, or use `unwrap_or`/`unwrap_or_default` for truly optional values. Leave the vendored `evtx-patched` reductions as a separate, longer-term fork-maintenance task.
8. **Introduce a repository SQL helper for dynamic IN/placeholder queries.** A helper that accepts `&[&str]` and returns a parameterized statement would remove the repeated `format!` blocks across `file_repo.rs`, `graph_repo.rs`, `timeline_repo.rs`, `notebook_repo.rs`, `batch_repo.rs`, and `audit_repo.rs`.

#### P2 — Hardening and polish

9. **Replace substring-based error classification.** Move error classification to the typed error enum level: add a `category()` method to service errors or implement `From<ConcreteError>` for `CommandError` so the mapping is explicit and compiler-checked.
10. **Add a module-size lint/fail to CI.** The project already has a 1500-line policy; add a lightweight check (e.g., `scripts/check-module-size.ps1`) to prevent further regressions.
11. **Consider running `cargo clippy --workspace --all-targets -- -D warnings` as a pre-merge gate.** While the project documents this as a default gate, the audit did not re-run it; ensure it passes on the current codebase to catch new lint regressions.
12. **Expand the SQL helper to all repositories and add a lint against new `format!` SQL.** Once the helper is in place, enforce it through a repository guard script to keep the SQL-composition surface from growing.

---

## 5. Frontend Code Quality Analysis

- The frontend is a **React 18 + TypeScript 5.x + Vite 6** application with strict TypeScript enabled, centralized path aliases (`@/`), and a clean separation between the API layer (`frontend/src/lib/api/`), feature hooks (`frontend/src/features/*/hooks.ts`), and Zustand stores (`frontend/src/stores/`).
- **No direct Tauri `invoke` calls leak outside the API layer**; every command is routed through `apiClient.request(...)` using centralized command constants in `frontend/src/lib/api/commands.ts`.
- **Server state is managed with TanStack Query** and **client state with Zustand**, with current Vitest coverage exceeding the configured thresholds across all four dimensions (lines, statements, functions, branches).
- Page components remain within the 500-line policy limit, but the largest page (`V3Dashboard.tsx`) is within 30 lines of the boundary, and several auxiliary files (`use-file-browser.ts`, `mcp-store.ts`) are large enough to warrant attention. The pages directory now contains 23 `.tsx` files (17 top-level pages plus 6 `settings/` sub-pages).

### Metrics Table

| Category | Metric | Observed Value | Notes |
|---|---|---|---|
| **Pages** | Non-test `.tsx` files in `frontend/src/app/pages/` | **23 (17 top-level + 6 settings/ sub-pages)** | Includes sub-pages under `settings/`; excludes helpers like `use-file-browser.ts`. |
| **Page size** | Largest page file | **471 lines** (`V3Dashboard.tsx`) | Under the 500-line component limit, but close to it. |
| **Pages > 500 lines** | Count | **0** | All page components comply with the policy. |
| **API layer** | Non-test files in `frontend/src/lib/api/` | **16** | Includes `client.ts`, `commands.ts`, and 14 domain files. |
| **API layer** | Test files in `frontend/src/lib/api/` | **15** | Good coverage of the API contract. |
| **Stores** | Zustand stores in `frontend/src/stores/` | **4** | `ui-store.ts`, `analysis-store.ts`, `mcp-store.ts`, `selection-store.ts`. |
| **Test files** | Total `*.test.{ts,tsx}` in `frontend/src` | **76** | Slightly higher than the 71/75 figures in earlier notes; reflects recent additions. |
| **Direct `invoke` usage** | Files outside `frontend/src/lib/api/` | **0** | Only `frontend/src/lib/api/client.ts` imports `invoke`. |
| **TanStack Query** | Files referencing `useQuery`/`useMutation`/`QueryClient` | **~204 matches** | Used consistently for server-state caching and invalidation. |
| **Coverage** | Lines / Statements / Functions / Branches | **65% / 64.37% / 60.36% / 55.61%** | Measured via `pnpm test:coverage` on audit date. Exceeds Vitest thresholds of 45% / 45% / 45% / 35%. |
| **Quality gates** | Latest lint / typecheck / guard scripts | **PASS** | `pnpm lint`, `pnpm typecheck`, and relevant PowerShell guard scripts pass. |

### Highlighted Strengths

1. **Centralized API and command contract.** `frontend/src/lib/api/client.ts` is the single place that calls `invoke`, and `frontend/src/lib/api/commands.ts` defines all 96-ish Tauri command names as typed constants. This makes renaming or auditing command usage straightforward and prevents string-typo regressions.

2. **Clean architecture layering.** Feature hooks (`frontend/src/features/case/hooks.ts`, `frontend/src/features/files/hooks.ts`, etc.) wrap TanStack Query, while UI components consume hooks and stores. The `apiClient` is never invoked from pages or components directly.

3. **TypeScript strictness.** `tsconfig.json` enables `strict: true`, `noEmit: true`, `isolatedModules: true`, `forceConsistentCasingInFileNames: true`, and bundler module resolution, providing a solid foundation for type safety.

4. **State-management separation.** Server state (cases, files, jobs, artifacts, search, timeline) is handled by TanStack Query with explicit cache invalidation (e.g., `qc.invalidateQueries({ queryKey: ['case'] })`), while local UI state (navigation, selection, analysis panel progress) lives in Zustand stores.

5. **Coverage above thresholds.** Current v8 coverage sits at **65% lines, 64.37% statements, 60.36% functions, 55.61% branches**, measured via `pnpm test:coverage` during the audit and comfortably above the 45/45/45/35 thresholds defined in `vitest.config.ts`.

6. **Small, focused layout components.** `frontend/src/components/layout/AppShell.tsx` is only 13 lines and merely composes `TopBar`, `BottomDrawer`, and children, demonstrating good layout/component decomposition.

### Risks and Issues Found

1. **Page size boundary pressure.** `frontend/src/app/pages/V3Dashboard.tsx` is **471 lines** and `frontend/src/app/pages/DataAnalysis.tsx` is **349 lines**. While both remain under the 500-line limit, V3Dashboard is close enough that adding a new dashboard section would push it over. The file is also JSX-heavy, making it harder to unit-test individual sections in isolation.

2. **Large auxiliary files in the pages directory.** `frontend/src/app/pages/use-file-browser.ts` is **464 lines**, and `frontend/src/app/pages/file-tree-utils.ts` is **48 lines**. These are not page components but co-located page helpers; the hook in particular mixes keyboard handling, selection, sorting, and virtual scrolling logic and could be split into smaller, reusable hooks or moved to `frontend/src/features/files/`.

3. **Large store file.** `frontend/src/stores/mcp-store.ts` is **311 lines** and contains MCP server connection, resource/tool/prompt caching, and error handling. This is a high-risk area for future regressions because it bundles transport, protocol, and UI state in one file.

4. **Low per-file coverage in several areas.** Although aggregate coverage passes, individual files are poorly exercised:
   - `frontend/src/app/pages/CaseActions.tsx` — **4.76% lines, 7.69% functions**.
   - `frontend/src/components/notebook/NotebookEntryForm.tsx` — **17.97% lines, 12.82% functions**.
   - `frontend/src/components/gql/GqlAutocomplete.tsx` — **0% coverage**.
   - `frontend/src/components/gql/GqlQueryInput.tsx` — **31.03% lines, 36.36% functions**.
   - `frontend/src/components/gql/GqlResultView.tsx` — **43.75% lines, 40% functions**.
   - Settings sub-sections (`McpSection.tsx`, `PreviewSection.tsx`, `ImportPerformanceSection.tsx`) are also under 40% lines.

5. **Very large test files.** Test files are exempt from the 500-line component limit, but three page tests exceed 600 lines:
   - `frontend/src/app/pages/V2Workbench.test.tsx` — **756 lines**.
   - `frontend/src/app/pages/DataAnalysis.test.tsx` — **675 lines**.
   - `frontend/src/app/pages/FileBrowser.test.tsx` — **627 lines**.
   Long tests tend to be brittle and slow to debug; they may benefit from shared fixtures or page-object helpers.

6. **Low coverage thresholds.** The Vitest thresholds (`lines: 45`, `statements: 45`, `functions: 45`, `branches: 35`) are permissive. While current coverage exceeds them, they do not strongly constrain future regressions, especially in branches.

7. **Tailwind 4 CSS-first migration is active but custom colors remain inline.** Several files (e.g., `V3Dashboard.tsx`) use arbitrary hex values like `bg-[#fafafa]` and `text-[#111]`. These are not theme-tokenized and could drift from the design system. Tailwind 4 is configured via `@tailwindcss/vite` without a `tailwind.config.js`, so design tokens should live in CSS theme files rather than inline hex literals.

### Improvement Recommendations

#### P0 — Address before next release

- **Split `frontend/src/app/pages/V3Dashboard.tsx`** before it crosses the 500-line component limit. Extract each dashboard section (Graph, Data Sources, Timeline, Artifacts, Correlation, Platform Coverage, Rule Packs, Batch Status) into small components under `frontend/src/components/dashboard/` or `frontend/src/app/pages/dashboard/`. This also improves testability and React re-render performance.
- **Add coverage for `CaseActions.tsx`** and the GQL notebook components (`GqlAutocomplete.tsx`, `GqlQueryInput.tsx`, `GqlResultView.tsx`). These are the lowest-covered production files and are the most likely to hide regressions.

#### P1 — Within the next sprint

- **Refactor `frontend/src/app/pages/use-file-browser.ts` (464 lines)** into focused hooks under `frontend/src/features/files/hooks.ts` or a dedicated `frontend/src/features/files/hooks/` directory, separating keyboard navigation, selection, sorting, and virtual-tree concerns.
- **Refactor `frontend/src/stores/mcp-store.ts` (311 lines)** into smaller stores or helper modules: one for MCP server configuration, one for connection state, and one for cached resources/tools/prompts. This reduces the blast radius of MCP transport changes.
- **Raise Vitest coverage thresholds** from `45/45/45/35` to at least `55/55/55/45` to prevent future regressions, aligning with the current actual coverage rather than leaving a large margin.
- **Standardize Tailwind color usage** by replacing inline arbitrary hex values in dashboard and analysis pages with theme CSS variables from `frontend/src/styles/theme.css` or `tailwind.css`.

#### P2 — Quality-of-life and technical debt

- **Create shared test fixtures / page objects** for the three oversized page tests (`V2Workbench.test.tsx`, `DataAnalysis.test.tsx`, `FileBrowser.test.tsx`) to reduce duplication and improve maintainability.
- **Add an ESLint rule or guard script** that warns when a production `.tsx` file exceeds 400 lines, giving an early warning before the 500-line hard limit is reached.
- **Document the frontend state-management convention** (TanStack Query for server state, Zustand for local UI state) in `AGENTS.md` or a frontend-specific `README.md` so new feature work follows the pattern consistently.
- **Consider adding a visual regression or component-story harness** for the low-covered UI components (settings sections, notebook forms, GQL widgets) to complement unit tests and catch unintended UI changes.

---

## 6. Architecture and Data Flow Analysis

### Summary

Forensics Workbench is a **backend-led, layered Tauri desktop application**. A Rust workspace of 38 crates (37 library crates plus the Tauri shell) performs evidence processing, while a React 18 frontend renders the investigator UI. The architecture enforces strict dependency direction: domain and transport crates sit at the bottom, parser/core crates feed application services, and Tauri commands act as thin IPC adapters. All state is centralized in `AppState`, and the frontend/backend contract is expressed through manually-synchronized DTOs and event topics in `crates/transport`.

This section analyzes the crate layering, the request/event paths, the DTO contract, the media-preview security boundary, and the MCP policy boundary. It identifies the project's architectural strengths and its principal long-term risk: the manual IPC contract between Rust and TypeScript.

### Architecture Layers

```mermaid
graph TB
    subgraph Frontend
        UI[React 18 + Vite + Tailwind 4]
        API[apiClient.request@frontend/src/lib/api/client.ts]
        EVT[EventBus@frontend/src/lib/events/bus.ts]
    end

    subgraph Tauri_Shell[apps/desktop/src-tauri]
        CMD[96 Tauri commands<br/>apps/desktop/src-tauri/src/commands/]
        STATE[AppState<br/>state::AppState]
        MEDIA[evidence-media:// protocol handler<br/>src/media_protocol.rs]
    end

    subgraph App_Services[crates/app-services]
        SVC[use-case services<br/>file_service, case_service, ...]
    end

    subgraph Transport[crates/transport]
        DTO[DTOs: 33 domain files<br/>src/dto/]
        EVT_RUST[EventTopic enum<br/>src/events/mod.rs]
        ERR[CommandError / ApiErrorDto]
        REQ[command request structs<br/>src/commands/mod.rs]
    end

    subgraph Persistence[crates/persistence-sqlite]
        REPO[15 repositories]
        MIG[33 migrations]
        DB[(SQLite + WAL)]
    end

    subgraph Core[Core / Parser Crates]
        EVID[evidence-core]
        FS[fs-ntfs / fs-fat / fs-exfat / fs-ext4 / fs-xfs / fs-btrfs / fs-apfs / fs-hfsplus]
        ART[artifacts-windows / linux / macos]
        SRC[search / timeline / catalog / exchange]
    end

    subgraph Domain[crates/domain]
        ENT[CaseId / FileEntryId / DataSourceId / Artifact / TimelineEvent]
    end

    UI --> API
    API --> CMD
    CMD --> SVC
    SVC --> DTO
    SVC --> REPO
    SVC --> EVID
    SVC --> FS
    SVC --> ART
    SVC --> SRC
    REPO --> DB
    EVID --> Domain
    FS --> Domain
    ART --> Domain
    SRC --> Domain
    DTO --> Domain
    CMD --> STATE
    CMD --> EVT_RUST
    EVT --> EVT_RUST
    MEDIA --> SVC
    MEDIA --> STATE
```

| Layer | Responsibility | Key files |
|---|---|---|
| Frontend | Investigator UI, API client, event subscriptions | `frontend/src/app/pages/`, `frontend/src/lib/api/client.ts`, `frontend/src/lib/events/bus.ts` |
| Tauri commands | Thin IPC adapters, input validation, state access, audit logging | `apps/desktop/src-tauri/src/commands/*.rs`, `apps/desktop/src-tauri/src/lib.rs` |
| Application services | Per-domain use-case orchestration | `crates/app-services/src/*_service.rs` |
| Transport | DTOs, command requests, event topics, error taxonomy | `crates/transport/src/dto/`, `crates/transport/src/commands/`, `crates/transport/src/events/` |
| Persistence | SQLite repositories, migrations, WAL pragmas | `crates/persistence-sqlite/src/repositories/*_repo.rs`, `src/migrations/scripts/*.sql` |
| Core / parsers | Evidence readers, filesystem parsers, artifact extractors, search/timeline engines | `evidence-core`, `fs-*`, `artifacts-*`, `search`, `timeline`, `catalog`, `exchange` |
| Domain | Core entities and identifiers | `crates/domain/src/` |

### Dependency Direction and Layering

The workspace follows a strict **inward-pointing dependency** rule:

1. `domain` and `transport` are the bottom layer. Every crate that serializes data or returns errors depends on `transport`.
2. `persistence-sqlite`, `evidence-core`, `fs-*`, `artifacts-*`, `search`, `timeline`, `catalog`, `reports`, and `exchange` depend on `domain` and sometimes `transport` for DTOs/errors, but **never** on Tauri or the frontend.
3. `app-services` consumes persistence and core crates, and returns `transport` DTOs.
4. Tauri commands (`apps/desktop/src-tauri`) consume `app-services` and `transport`, and are the only layer that touches the Tauri runtime and the desktop window state.

This direction was verified by inspecting `Cargo.toml` files for representative parser crates:

- `crates/evidence-core/Cargo.toml` depends on `serde`, `chrono`, `thiserror`, `anyhow` — no Tauri or frontend.
- `crates/fs-ntfs/Cargo.toml` depends on `evidence-core`, `serde`, `chrono`, `thiserror` — no Tauri or frontend.
- `crates/artifacts-windows/Cargo.toml` depends on `domain`, `artifacts-core`, `evtx`, `rusqlite`, crypto crates — no Tauri or frontend.

The rule is also enforced by repository guard scripts, notably `check-command-sql-boundary.ps1` and `check-media-protocol-guard.ps1`, which prevent raw SQL from leaking into command handlers and ensure media preview stays on the `evidence-media:` protocol.

### DTO and Event Contract

#### DTOs

The frontend/backend contract is defined in `crates/transport/src/dto/`. `crates/transport/src/dto/mod.rs` re-exports 33 domain modules and hundreds of DTO types, all following the convention:

- Type names end in `Dto` on the Rust side (e.g., `FileEntryRowDto`).
- `#[serde(rename_all = "camelCase")]` serializes fields for TypeScript.
- `#[serde(skip_serializing_if = ...)]` omits optional/false values.

The TypeScript mirror lives in `frontend/src/types/`, re-exported from `frontend/src/types/models.ts`. This is a **manual mirror**: there is no code generation. Adding a field to `crates/transport/src/dto/files.rs` without updating `frontend/src/types/files.ts` will compile on both sides but can fail at runtime.

#### Events

Backend→frontend events are typed in `crates/transport/src/events/mod.rs`:

- 18 string constants (`TOPIC_*`) and a matching `EventTopic` enum.
- `EventTopic` serializes as kebab-case (`"job-progress"`, `"import-phase-progress"`, etc.).
- `EventEnvelope<T>` wraps every event with `event_id`, `topic`, `ts`, and `payload`.

The frontend mirror is `frontend/src/types/events.ts`:

```typescript
export type EventTopic =
  | 'case-opened'
  | 'case-closed'
  | 'job-created'
  | 'job-started'
  | 'job-progress'
  ...
```

The event contract is tested in Rust (e.g., `event_topic_serializes_as_wire_topic`), but there is no automated check that the TypeScript union stays in sync with the Rust enum.

#### Command Requests

Request DTOs live in `crates/transport/src/commands/mod.rs`. They carry validation methods such as `validate()` and `validate(&mut self)`, and enforce constraints like:

- `MAX_PAGE_LIMIT = 500` for file/search/timeline pagination.
- Import source paths reject Windows device paths (`\\.\`, `\\?\`), null bytes, and reserved device names.
- Export destination paths reject device paths and default to `overwrite = false`.

### Request Flow Walkthrough

The file-browsing path is representative of the full request flow.

#### 1. Frontend API client

`frontend/src/lib/api/client.ts` wraps Tauri's `invoke` and normalizes errors to `ApiErrorDto`:

```typescript
class ApiClient {
  async request<T>(command: string, payload?: Record<string, unknown>) {
    try {
      return await invoke<T>(command, payload);
    } catch (error) {
      throw toApiError(error, `COMMAND_${command.toUpperCase()}_FAILED`);
    }
  }
}
```

Pages and hooks must use `apiClient.request`; direct `invoke` calls are not allowed by project convention.

#### 2. Tauri command

`apps/desktop/src-tauri/src/commands/file_commands.rs` exposes commands such as `get_file_rows_request`. Each command:

1. Validates the request (`request.validate()`).
2. Locks `AppState` and checks for an active case.
3. Opens a fresh SQLite connection via `command_support::get_case_connection`.
4. Spawns the blocking work onto `tauri::async_runtime::spawn_blocking`.
5. Delegates to `app_services::file_service` and maps service errors to `CommandError`.

Example excerpt from `get_file_rows_request`:

```rust
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    mut request: GetFileRowsRequest,
) -> Result<FileRowsPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = get_case_connection(&app_state)?;
        file_service::get_file_rows_for_request(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
```

#### 3. Application service

`crates/app-services/src/file_service/mod.rs` is the public entry point. It re-exports sub-modules for tree queries, file rows, enumeration, export, MFT handling, and preview. For example, `get_file_rows_for_request` sorts the full result set and then paginates, while `get_file_jump_context` resolves a target file's directory context and ancestor chain.

#### 4. Repository / SQLite

The service calls repositories such as `persistence_sqlite::repositories::file_repo::FileRepo`. The persistence layer runs 33 migration scripts and opens each connection with WAL pragmas (`journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout=30000`, `synchronous=NORMAL`).

#### 5. Error return path

Service errors are typed with `thiserror` (e.g., `FileServiceError`). The command layer converts them through `CommandError::from_service_error` into `ApiErrorDto` with `code`, `message`, `category`, `details`, and `recoverable`. The frontend receives a structured error that the UI can render or retry on.

### Event Flow Walkthrough

1. Backend services and task manager emit typed events using constants from `crates/transport/src/events/mod.rs`.
2. Tauri pushes these events to the webview.
3. Frontend `EventBus` (`frontend/src/lib/events/bus.ts`) subscribes by `EventTopic` and routes payloads to listeners or Zustand stores.

Event topics cover case lifecycle, job lifecycle, import progress, artifact additions, timeline updates, search index progress, and cache status. The 18 topics are listed in `crates/transport/src/events/mod.rs` and mirrored in `frontend/src/types/events.ts`.

### Security Boundaries

#### Read-only evidence

The core forensic invariant is that **original evidence sources are never modified**. All writes are limited to:

- The case workspace directory.
- The SQLite case database.
- Search/timeline index directories.
- Explicit user export paths.

Import and preview operations open evidence readers in read-only mode. File extraction writes to a destination chosen by the investigator, with `overwrite=false` by default and a conflict returned if the target exists.

#### Path validation

Path validation happens at the command boundary before any I/O:

- `validate_import_source_path` rejects null bytes, `\\.\` device paths, `\\?\` extended paths, and reserved Windows names (`CON`, `PRN`, `AUX`, `NUL`, `COM*`, `LPT*`).
- `safe_relative_path` in `file_service` rejects `..`, URL-encoded traversal, absolute paths, and null bytes.
- `validate_export_destination_path` rejects device paths for exports.

#### Media protocol

Media preview is the most sensitive path because it exposes evidence bytes to the webview. The design avoids leaking host filesystem paths:

- `get_media_url` returns either a bounded `data:` URL (for files under `MAX_INLINE_MEDIA_PREVIEW_BYTES`) or an `evidence-media://handle/<encoded>` URL.
- The custom protocol handler (`apps/desktop/src-tauri/src/media_protocol.rs`) resolves the encoded handle through the runtime cache, validates that the handle belongs to the active case, and streams a bounded byte range.
- The CSP in `tauri.conf.json` explicitly allows `media-src 'self' data: evidence-media:`; no `file:` or `asset://` fallback is permitted.
- Range requests are clamped to `MAX_VIEWER_RANGE_LENGTH` (1 MB) and validated against the file size before reading.

#### MCP policy

`AppState` holds MCP configuration and live clients. The default permission profile is least-privilege:

- `resourceAccess = readOnly`
- `toolAccess = disabled`
- `promptAccess = readOnly`
- `networkPolicy = localhostOnly`

MCP server configurations are validated on load/save, stale clients are pruned when the config changes, and SSE transports only allow `http/https` without embedded credentials. MCP outputs entering the UI or reports must preserve source boundaries.

### Strengths

1. **Strict layering and dependency direction**. Parser and core crates have no Tauri or frontend dependencies, making them testable in isolation and reusable outside the desktop app.
2. **Thin command layer**. Tauri commands are focused on validation, state access, and delegation; business logic lives in `app-services` and below.
3. **Centralized state**. `AppState` owns the active case, task manager, MCP clients, and runtime cache. A single mutex per concern prevents accidental cross-case races and makes resource lifetimes explicit.
4. **Typed, structured errors**. `ApiErrorDto` crosses the IPC boundary with forensic categories (`validation`, `parser`, `security`, `external`, `timeout`, `internal`) and a `recoverable` flag, rather than raw strings or stack traces.
5. **Defense-in-depth media preview**. The combination of scoped handles, a custom protocol, CSP allow-listing, and bounded range reads keeps evidence paths out of the frontend.
6. **Repository guard scripts**. Automated checks (`check-command-sql-boundary`, `check-media-protocol-guard`, `check-release-guard`) encode architectural and security boundaries in CI.

### Risks

1. **Manual DTO/event synchronization is the single largest source of drift risk**. There is no code generation or contract test linking `crates/transport/src/dto/` to `frontend/src/types/`. A renamed Rust field that is not mirrored in TypeScript will fail at runtime.
2. **EventBus and transport event types are manually mirrored**. `frontend/src/types/events.ts` must match every Rust `EventTopic` variant and payload shape.
3. **AppState mixes concerns**. It holds case state, task management, MCP state, and runtime cache in one struct. While currently manageable, this centralizes too many dependencies as the surface grows.
4. **Error classification is partially string-based**. `CommandError::from_service_error` maps some service errors to categories by inspecting error messages, which is brittle for new error variants.
5. **No isolated worker process**. Long-running tasks are managed by `TaskManager` but still run on the Tauri-managed Tokio runtime; a CPU-bound ingest or indexing job can starve the UI command handler.
6. **MCP is a controlled extension channel with elevated trust**. A misconfigured MCP server (e.g., `toolAccess` elevated, `networkPolicy` relaxed) could expose the host to arbitrary tool execution or external network access.

### Improvement Recommendations

#### P0 — Before V2 release

1. **Add an IPC contract regression test**. Generate JSON samples for every Rust DTO and event payload, and assert that the TypeScript types and runtime validators in `frontend/src/types/` accept them. This can be a CI step that runs a small Rust exporter and a TypeScript parser.
2. **Freeze the event topic list and document payloads**. The 18 topics are sufficient for V2; add a `docs/ipc-event-contract.md` table listing each topic, its Rust payload type, and its TypeScript shape.
3. **Add a guard script that detects DTO drift**. Compare exported Rust DTO JSON schema (or field names) against the TypeScript interfaces and fail CI on mismatch.

#### P1 — Near-term engineering debt

4. **Refactor `AppState` into focused sub-states** (e.g., `CaseState`, `TaskState`, `McpState`, `CacheState`) to reduce the cross-cutting dependency surface and improve testability.
5. **Replace substring-based error classification** with explicit `category()` methods on each `thiserror` service enum or with `From<ConcreteError>` for `CommandError` implementations.
6. **Add command audit logging** for security-sensitive operations: case create/delete, file extract, data-source delete, and MCP tool calls. The audit trail should include the case ID, action, resource ID, and outcome.
7. **Introduce typed command wrappers in the frontend**. Instead of `apiClient.request('get_file_rows_request', payload)`, generate per-command functions so the payload and return types are checked at compile time.

#### P2 — Hardening and polish

8. **Evaluate a lightweight schema generator** such as `ts-rs` or `typeshare` for the core DTOs. If acceptable, generate TypeScript interfaces from Rust to eliminate manual mirroring; if not, document the explicit "no-codegen" policy and the drift-detection guard script.
9. **Document and test the media handle lifecycle**. Ensure handles are invalidated when the active case is closed or when the case workspace is removed, so stale handles cannot be reused.
10. **Consider isolating long-running tasks**. For V3 scheduling, evaluate a dedicated worker thread pool or a separate ingest process so that heavy indexing/artifact extraction cannot block the Tauri command loop.

---

## 7. Deep Dive — Search and Catalog Indexing

### Summary

- **Full-text search is implemented on top of Tantivy** in `crates/search`. The public surface is small: `search::SearchIndex` wraps the index, `search::extract_text` reads file bytes, and `search::highlight` produces snippets. `crates/search/src/lib.rs:1-9` re-exports these three capabilities.
- **The index schema is intentionally minimal**: `file_id` (STRING|STORED), `path` (TEXT|STORED), `content` (TEXT|STORED), and `name` (TEXT|STORED). `crates/search/src/indexer/tantivy_writer.rs:52-55`.
- **Indexing happens during the artifact-analysis staging merge**, not inside the abstract `ingest` pipeline. Each import worker writes extracted text into a per-worker staging SQLite table; after workers finish, `merge_analysis_staging_to_main` copies those rows into the case Tantivy index at `<case_root>/indexes/tantivy`.
- **Automatic indexing is tightly budgeted**: only files with extensions `txt`, `log`, `csv`, `json`, `xml`, `html`, `htm`, or `md` are considered, each up to 256 KiB, and only the first 100 qualifying files across all workers are indexed. `crates/infrastructure/src/constants.rs:8,11`.
- **The `catalog` crate is dead code**: `crates/catalog/src/lib.rs:1-9` marks it **DEPRECATED** and states it has no production consumers. The in-memory `ExtensionProjection` and `PathPrefixProjection` are not wired into the import or search flows.
- **Frontend Search is a thin query console**: `frontend/src/app/pages/Search.tsx` uses `useSearchResults` (TanStack Query) to call `searchFiles`; the default placeholder query is SQL-like, but the backend is a raw Tantivy `QueryParser`.

### Key Components

| Component | Role | Key file |
|---|---|---|
| `search::SearchIndex` | Tantivy index wrapper (create, open, index, search) | `crates/search/src/indexer/tantivy_writer.rs` |
| `search::extract_text` | Text extraction (UTF-8/UTF-16 BOM, 10 MiB cap, binary skip) | `crates/search/src/extractor/text_extractor.rs` |
| `search::highlight` | Custom snippet/highlighter used at query time | `crates/search/src/highlighter/mod.rs` |
| `search_service` | Service orchestration, DTO mapping, instrumentation | `crates/app-services/src/search_service.rs` |
| Analysis worker runtime | Extracts text and stages `index_docs` rows | `crates/app-services/src/import_analysis/worker_runtime.rs` |
| Analysis staging merge | Merges staged text rows into the Tantivy index | `crates/app-services/src/staging/analysis_merge.rs` |
| Search commands | Validates query, resolves index path, records provenance | `apps/desktop/src-tauri/src/commands/search_commands.rs` |
| Frontend Search page | Query input, results table, inspector | `frontend/src/app/pages/Search.tsx` |
| Frontend search hook | TanStack Query wrapper | `frontend/src/features/search/hooks.ts` |
| IPC DTOs | `SearchHitDto`, `SearchSnippetDto`, `SearchResultPageDto` | `crates/transport/src/dto/search.rs` |

### How Indexing Works: Ingest-Time vs Query-Time

#### Ingest-time pipeline

The `crates/ingest` crate defines an abstract `IngestPipeline` and `IngestSink` but does **not** itself build the search index. The actual indexing path lives in `crates/app-services/src/import_analysis/`:

1. **Worker extraction** (`worker_runtime.rs:180-208`): For every file that passes `should_index_file`, the worker reads the first 256 KiB (`IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES`), calls `search::extract_text`, and, if the result is extractable and non-empty, pushes an `IndexDocRow` into a per-worker staging SQLite table `index_docs`. The global atomic counter `shared.indexed_total` stops the process once 100 files have been indexed (`IMPORT_TEXT_INDEX_LIMIT`).
2. **Staging flush** (`worker_runtime.rs:332-355`): `index_docs` are inserted with `INSERT OR REPLACE` in batches of 25 (`INDEX_DOC_INSERT_BATCH`).
3. **Staging merge** (`analysis_merge.rs:12-96`): After all workers finish, `merge_analysis_staging_to_main` iterates over each worker database. For each worker it first merges artifact/timeline rows into the main case database, then calls `merge_one_analysis_index_docs`.
4. **Index merge** (`analysis_merge.rs:126-172`): Opens or creates the Tantivy index at `<case_root>/indexes/tantivy`, paginates through staging rows 50 at a time (`INDEX_DOC_MERGE_PAGE_SIZE = 50`), builds `search::ExtractedText` values, and calls `SearchIndex::index_documents`.

#### Index-time behavior

- `index_documents` deletes any existing document with the same `file_id` before adding the new one (`tantivy_writer.rs:108`), preventing duplicate hits when a file is re-analyzed.
- Binary or empty documents are skipped (`tantivy_writer.rs:110-112`).
- The `name` field is derived from the last path component at index time (`tantivy_writer.rs:114-117`).
- `index_files_chunked` provides per-1000-document commits for long-running indexing, but the production merge path uses the single-commit `index_documents`.

#### Query-time pipeline

1. The frontend calls `searchFiles(query, offset, limit)` → `apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, …)` (`frontend/src/lib/api/search.ts:11-16`).
2. The Tauri command `search_files_request` (`search_commands.rs:29-111`) validates the request, checks `MAX_QUERY_LENGTH` (1000 characters), resolves the active case, and verifies the index directory exists.
3. `search_files_real_instrumented` (`search_service.rs:185-202`) opens the Tantivy index and calls `SearchIndex::search`.
4. `SearchIndex::search` (`tantivy_writer.rs:132-201`) parses the query with Tantivy `QueryParser` over the `content` field. If parsing fails, it falls back to a phrase-quoted version of the raw input. It collects `TopDocs` ordered by score plus a `Count` for the total hit count.
5. Snippets are generated by the custom `highlight` function (`highlighter/mod.rs:8-62`), which lowercases both content and query, finds term positions, clusters nearby matches, and returns up to 5 snippets of ≤512 bytes each.
6. Results are mapped to `SearchResultPageDto` (`transport/src/dto/search.rs:26-32`) and emitted to the frontend.

### Search Index Structure and Query Capabilities

#### Schema

`crates/search/src/indexer/tantivy_writer.rs:52-55`:

```rust
schema_builder.add_text_field("file_id", STRING | STORED);
schema_builder.add_text_field("path", TEXT | STORED);
schema_builder.add_text_field("content", TEXT | STORED);
schema_builder.add_text_field("name", TEXT | STORED);
```

- `file_id` is the document key (STRING, not tokenized, stored for retrieval).
- `content` is the only field searched by `QueryParser` (`tantivy_writer.rs:153`). `path` and `name` are stored but not part of the default query.
- `path` and `name` are tokenized as TEXT, so they could be searched with explicit field syntax, but the frontend does not expose that.

#### Query capabilities

- **Default field**: `content`.
- **Syntax**: Tantivy QueryParser syntax (e.g., `keyword`, `"exact phrase"`, `keyword1 AND keyword2`, `content:keyword`).
- **Fallback**: if the parser rejects the input, the whole string is escaped and re-parsed as a phrase query (`"…"`). `tantivy_writer.rs:157-162`.
- **Limits**: `MAX_QUERY_LENGTH = 1000` characters; pagination is capped at `offset + limit ≤ 1000` results (`search_service.rs:135`).

#### Highlighting

Highlighting is **not** Tantivy's built-in snippet generator. The custom implementation in `highlighter/mod.rs`:

- Caps scanned content at 256 KiB (`MAX_HIGHLIGHT_CONTENT_BYTES`).
- Splits the query on whitespace, lowercases terms, and finds all positions in the lowercased content.
- Clusters match positions within `SNIPPET_RADIUS * 2 = 120` bytes.
- Returns up to `MAX_SNIPPETS = 5` snippets, each ≤ `MAX_SNIPPET_BYTES = 512` bytes.
- Highlight offsets are byte offsets within the snippet text, not the original document.

This is simple and deterministic, but it does not support phrase queries or proximity matching in snippets, and UTF-16 content is converted to UTF-8 before indexing so offsets are byte offsets in the reconstructed string.

### Catalog Projections (Extension, Path Prefix) and Their Use

The `catalog` crate (`crates/catalog`) provides in-memory projections that are **not currently used** in production. The crate's own `lib.rs:1-9` declares:

> **DEPRECATED**: This crate currently has no consumers in the production codebase. The cataloging functionality has been absorbed into the import pipeline at `apps/desktop/src-tauri/src/commands/import/pipeline.rs`. Retained for reference; scheduled for removal in a future cleanup pass.

#### ExtensionProjection

`crates/catalog/src/projection/mod.rs:5-35`:

```rust
pub struct ExtensionProjection {
    index: HashMap<String, Vec<FileEntryId>>,
}
```

- `build(entries)` groups `FileEntryId`s by `entry.ext` (or `""` when no extension).
- `query(ext)` returns `&[FileEntryId]` for the given extension.
- `extensions()` returns all extension keys.

#### PathPrefixProjection

`crates/catalog/src/projection/mod.rs:37-75`:

```rust
pub struct PathPrefixProjection {
    index: Vec<(String, Vec<FileEntryId>)>,
}
```

- `build(entries, prefixes)` creates one bucket per requested prefix, then adds any entry whose `path.starts_with(prefix)` to each matching bucket.
- Prefixes are sorted alphabetically in the internal `Vec`.
- `query(prefix)` returns `&[FileEntryId]` for the exact prefix string.

#### CatalogIndex

`crates/catalog/src/indexing/mod.rs:6-54` wraps `ExtensionProjection` and stores `total_entries`. It also offers `build_with_prefixes(entries, prefixes)` to return both a `CatalogIndex` and a `PathPrefixProjection`.

#### Production status

- No crate declares `catalog` as a dependency.
- No Tauri command, service, or import step calls `CatalogIndex`, `ExtensionProjection`, or `PathPrefixProjection` outside the catalog crate's own tests.
- The catalog's functionality is conceptually replaced by the SQLite `file_entries` table and the `catalog` extension/path-prefix views that the file browser queries directly from the database.

### Frontend Search UI and Hook Integration

#### Hook

`frontend/src/features/search/hooks.ts:1-9`:

```typescript
export function useSearchResults(query: string) {
  return useQuery({
    queryKey: ['search', query],
    queryFn: () => searchFiles(query),
  });
}
```

The hook is a minimal TanStack Query wrapper. It fetches a single page of 50 results (`searchFiles` defaults to `offset=0, limit=50`) and caches by the full query string.

#### API layer

`frontend/src/lib/api/search.ts:11-17`:

```typescript
export async function searchFiles(
  query: string,
  offset: number = 0,
  limit: number = 50,
): Promise<SearchResultPage> {
  return apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, { request: { query, offset, limit } });
}
```

#### Page

`frontend/src/app/pages/Search.tsx:19-252` provides:

- A query input with placeholder `files WHERE extension IN ('.doc', '.xls') AND size > 10MB` and an "执行" button.
- Saved-query management (save, load, delete) persisted to local storage via `@/lib/saved-queries`.
- A `DenseDataTable` showing `path`, `score`, and the first snippet for each hit.
- An inspector pane for the selected hit, including a button to open the file in the file browser.
- Score filtering UI (`highScoreHits` is the count of items with `score >= 0.8`).

#### Type mismatch

The UI placeholder suggests a SQL-like query language, but the backend is a raw Tantivy `QueryParser`. There is no query translation layer; whatever the user types is passed directly to Tantivy. This is a user-experience risk.

### Performance and Correctness Notes

#### Performance

- **Index writer buffer**: 15 MB (`tantivy_writer.rs:77`).
- **Chunk commit interval**: 1000 documents (`CHUNK_COMMIT_INTERVAL = 1000`), but production merge uses `index_documents` with a single commit per 50-row page.
- **Benchmark thresholds**: `docs/benchmark-baseline.md:98-102` sets medium search hot query p95 ≤ 1.5 s and large search hot query p95 ≤ 4 s. These are governance thresholds; actual benchmark data is stored in `testdata/governance/v2-benchmark-baseline.json`.
- **Instrumentation**: both `index_files_instrumented` and `search_files_real_instrumented` emit `PerformanceReportDto` metrics (`search_service.rs:105-124` and `185-202`), which the command emits via `event_bridge::emit_performance_report_ready` and records as an investigation step for provenance.

#### Correctness

- **Document replacement**: `index_documents` deletes by `file_id` before re-adding, so re-indexing a file does not produce duplicates. `tantivy_writer.rs:526-560` tests this.
- **Incremental indexing**: `index_files_incremental` skips `file_id`s already present in the index, but the production merge path does not use it.
- **Partial index visibility**: `index_documents` commits after each call, so a multi-batch merge produces partially searchable results. `tantivy_writer.rs:705-757` tests this.
- **Highlight correctness**: The custom highlighter is case-insensitive and UTF-8-safe (`floor_char_boundary` ensures no multi-byte character is split), but it does not tokenize; it matches substrings.
- **Text extraction limits**: 10 MiB cap per file, UTF-16 BOM detection, and binary skip based on a conservative MIME-type allow-list. No extraction of PDF, Office, email bodies, or archives.

### Strengths

1. **Typed errors in search**. `IndexError` uses `thiserror` and covers Tantivy, IO, query, and schema errors (`tantivy_writer.rs:13-25`).
2. **Document replacement before indexing**. Prevents duplicate search hits when a file is re-analyzed.
3. **Incremental index reopening**. `SearchIndex::create` calls `Index::open_or_create`, so the same directory can be appended across multiple merge runs.
4. **Performance instrumentation**. Both indexing and search produce structured `PerformanceReportDto` metrics.
5. **Provenance integration**. Every search is recorded as an investigation step with query, offset, limit, and total hits (`search_commands.rs:86-105`).
6. **Custom highlighter is deterministic and safe**. No unsafe slicing, UTF-8 boundaries are respected, and it handles large repeated content gracefully.

### Risks and Issues Found

#### 1. Search indexing is coupled to artifact analysis staging

A user cannot search file contents immediately after importing a disk image; search results only appear after the artifact analysis phase has run and its staging DB has been merged. The abstract `ingest` crate (`crates/ingest`) has no search/catalog awareness, so the indexing step is a side-effect of the analysis subsystem rather than a first-class post-import operation.

#### 2. `catalog` crate is dead code

`crates/catalog/src/lib.rs` is marked **DEPRECATED** and has no production consumers. Per project rules, dead code should be removed; this crate is a cleanup candidate.

#### 3. Limited text extraction coverage

`extract_text` only handles plain text and UTF-16 text files. It does not extract text from PDF, Microsoft Office, PST/OST email bodies, or archives. As a result, the search index covers only a small fraction of the evidence set, and the default placeholder query (`files WHERE extension IN ('.doc', '.xls')`) advertises capabilities that are not actually indexed.

#### 4. SQL-like query hint is misleading

The Search page placeholder suggests a SQL-like query language, but the backend uses raw Tantivy `QueryParser`. Users may enter unsupported syntax and get confusing results or zero hits.

#### 5. No index lifecycle management

The Tantivy index directory is tied to the case root (`<case_root>/indexes/tantivy`), but there is no explicit cleanup, rebuild, or integrity-check API. If the index becomes corrupted, there is no documented recovery path.

#### 6. Search result pagination is capped at 1000 total hits

`search_service.rs:135` computes `search_limit = (offset + limit).min(1000)`. The frontend currently always requests the first page, so this is not yet visible, but it limits deep pagination and any future "show all results" feature.

#### 7. Highlight offsets are snippet-local, not document-local

`SearchHighlightDto` offsets are byte offsets within the snippet text, not within the original document. This is acceptable for the current UI but may confuse consumers expecting global offsets.

### Improvement Recommendations

#### P0 — Before V2 release

1. **Clarify the search query language in the UI**. Replace the SQL-like placeholder with a Tantivy query example (e.g., `"content:keyword AND name:invoice"`) or add a small syntax help panel. The current placeholder advertises unsupported functionality.
2. **Remove or deprecate the `catalog` crate** from the workspace if it truly has no consumers, to reduce build, dependency, and cognitive surface.
3. **Document the index coverage limits**. The UI and user manual should clearly state that full-text search only covers plain-text files (`.txt`, `.log`, `.csv`, `.json`, `.xml`, `.html`, `.htm`, `.md`) up to 256 KiB and limited to the first 100 qualifying files per import.

#### P1 — Near-term engineering debt

4. **Decouple full-text indexing from artifact analysis**. Add a lightweight post-import indexing step that extracts text from all plain-text files in the file catalog, so search is useful even before artifact analysis runs. This would also make the 100-file limit a soft cap rather than an artifact-analysis side effect.
5. **Add index health/rebuild API**. Expose a command to check index integrity and rebuild it from the case database, surfaced in the Settings or CaseOverview page.
6. **Expand extraction coverage**. Integrate a content-extraction pipeline for common formats (at minimum PDF and Office documents) or clearly document the limitation. Until then, consider removing the `.doc` / `.xls` placeholder.
7. **Add a dedicated `search` command test in the Tauri command suite** that exercises the query-length validation and the "no active case" empty-result path.

#### P2 — Hardening and polish

8. **Add search result regression tests** with a small fixture index and a known set of queries, so Tantivy upgrades do not silently break query behavior or scoring.
9. **Consider moving the search index into a sub-directory of the case cache** rather than the case root, and include it in backup/restore planning.
10. **Evaluate replacing the custom highlighter** with Tantivy's snippet generator or a token-aware highlighter to support phrase queries and document-global offsets.
11. **Raise or make configurable the 1000-result pagination cap** once deep pagination is needed.

---

## 8. Deep Dive — Timeline Generation

### Summary

- Timeline generation is a **dual-path pipeline**: (1) filesystem enumeration produces raw MACB events via `timeline::project_file_macb`; (2) artifact extractors emit additional events (prefetch, LNK, browser, EVTX, registry, Recycle Bin) through the `ArtifactSink` interface.
- The `timeline` crate is deliberately minimal: a single `lib.rs` (585 lines) converts `FileEntry` timestamps into `TimelineEvent` domain objects, filtering Unix-epoch sentinels and preserving deleted files' available timestamps.
- `TimelineService` (`crates/app-services/src/timeline_service.rs`) orchestrates two MACB projection strategies: an in-memory Rayon path (`project_and_store_macb`) and a SQL bulk-projection path (`ensure_macb_timeline_projected`), guarded by a `timeline_projection_meta` lock to ensure idempotency.
- The `TimelineEvent` domain model uses `source_object_id` as the primary correlation key; the same UUID links file entries, artifacts, timeline events, and graph nodes/edges.
- The frontend `Timeline` page renders a 60-bucket histogram, a paginated event table, and filter controls for time range and event type; events are fetched via TanStack Query and `apiClient.request`.
- Aggregation (`query_timeline_aggregated`) groups events by `(event_type, description)` into stripes/clusters, but is not exposed in the frontend today.

### 1. TimelineEvent Model and DTO

#### Domain entity

`crates/domain/src/timeline/mod.rs` defines the canonical Rust entity:

```rust
pub struct TimelineEvent {
    pub id: TimelineEventId,
    pub source_object_id: String,   // correlation key → file entry or artifact
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub parser_id: Option<String>,
    pub parser_version: Option<String>,
    pub confidence: Option<f32>,
    pub source_attribution: Option<String>,
    pub attrs: BTreeMap<String, Value>,
}
```

#### DTO

`crates/transport/src/dto/timeline.rs` mirrors the entity for IPC with `#[serde(rename_all = "camelCase")]`. Optional provenance fields are `skip_serializing_if` omitted. Three DTOs support the API:

- `TimelineEventDto` — single event.
- `TimelineClusterDto` — grouped events sharing `(event_type, description)` with `count`, `first_ts`, `last_ts`, and up to 5 sample IDs.
- `TimelineStripeDto` / `TimelineAggregatedDto` — server-side aggregation keyed by `event_type`.

#### Frontend TypeScript type

`frontend/src/types/timeline.ts`:

```typescript
export interface TimelineEvent {
  id: string;
  sourceObjectId: string;
  eventType: string;
  ts: string;
  title: string;
  description: string;
  parserId?: string;
  parserVersion?: string;
  confidence?: number;
  sourceAttribution?: string;
  attrs: Record<string, unknown>;
}
```

### 2. How Timeline Events Are Generated from Artifacts

There are two insertion paths into `timeline_events`.

#### 2.1 File MACB projection (filesystem path)

`crates/timeline/src/lib.rs::project_file_macb` takes a `FileEntry` and emits up to four events:

| FileEntry field | Event type | Title/description |
|---|---|---|
| `created_at` | `FILE_CREATED` | "File created: `{name}`" |
| `modified_at` | `FILE_MODIFIED` | "File modified: `{name}`" |
| `accessed_at` | `FILE_ACCESSED` | "File accessed: `{name}`" |
| `changed_at` | `FILE_METADATA_CHANGED` | "File metadata changed: `{name}`" |

- Unix-epoch timestamps are filtered out via `is_epoch`.
- Deleted files still project their available MACB timestamps; there is no synthetic `FILE_DELETED` event.
- `parser_id` is set to `"timeline.macb"`, `source_attribution` to the event type.
- `source_object_id` is set to `file.id.0`.

During import, `worker_runtime.rs` (`run_analysis_worker`) calls `timeline::project_file_macb` per file when `enable_timeline_projection` is true, and flushes events to the staging `timeline_rows` table. `timeline_service.rs` also exposes `project_and_store_macb` for callers that already have a `FileEntry` slice in memory.

#### 2.2 Artifact extractor path

Artifact extractors consume `ArtifactContext` and write to `dyn ArtifactSink`. The sink interface (`crates/artifacts-core/src/lib.rs`) supports both `write_artifact` and `write_timeline_event`. `new_timeline_event` fills `source_object_id` from `ctx.file_id`, generating a random UUID for the event ID.

Examples found in the code base:

- **Prefetch** (`crates/artifacts-windows/src/prefetch/parser.rs`): each parsed run time yields a `PROGRAM_EXECUTION` event with the executable name in `attrs`.
- **LNK** (`crates/artifacts-windows/src/lnk/parser.rs`): creation/access/write FILETIMEs produce `LINK_CREATED`, `LINK_ACCESSED`, `LINK_MODIFIED` events.
- **Recycle Bin** (`crates/artifacts-windows/src/recycle_bin/parser.rs`): deleted-file timestamps produce timeline events.
- **Registry** (`crates/artifacts-windows/src/registry/parser.rs`): user-assist timestamps and other dated registry values produce timeline events.
- **Browser** (`crates/app-services/src/analysis_service/extraction/browser.rs`): `BROWSER_VISIT` and `BROWSER_DOWNLOAD` events are created from Chrome/Firefox history and downloads; `source_object_id` is `candidate.file_id` (the browser database file).
- **EVTX** (`crates/app-services/src/analysis_service/extraction/evtx.rs`): Windows event log entries with `SystemTime` are converted to timeline events.

The analysis extraction orchestrator (`crates/app-services/src/analysis_service/extraction/mod.rs::run_analysis_extraction`) collects `timeline_events` from every extractor and inserts them in a single batch via `TimelineRepo::insert_batch_with_case`, making it the single write path for non-MACB artifact events.

### 3. MACB Semantics and Aggregation

#### 3.1 MACB semantics

The implementation is straightforward: each non-null filesystem timestamp becomes one event. The code does **not** currently distinguish between classic MACB (`M` = modified, `A` = accessed, `C` = created/MFT changed, `B` = birth) and the NTFS `$STANDARD_INFORMATION` vs. `$FILE_NAME` semantics; it simply maps the four columns from `file_entries` to the four event types. This is a pragmatic, filesystem-agnostic approach but may conflate MACB meanings across different file systems (NTFS, FAT, ext4, XFS, APFS, etc.).

Tests in `crates/timeline/src/lib.rs` cover:
- four events when all timestamps are present;
- zero events when none are present;
- epoch filtering;
- deleted files preserving available timestamps;
- directories treated identically to files;
- deterministic output (excluding random IDs);
- future timestamps accepted.

#### 3.2 SQL bulk MACB projection

`ensure_macb_timeline_projected` is the lazy/idempotent entry point used by queries. It creates a `timeline_projection_meta` table and, if not already done, runs `project_macb_timeline_sql`. This path builds deterministic IDs as `macb:{file_id}:{event_type}` and uses `INSERT OR IGNORE` with a deduplication subquery based on `(source_object_id, event_type, ts)`. This prevents duplicate MACB rows even if the function is called again.

After projection, `populate_timeline_event_graph` creates `TimelineEvent` graph nodes and `References` edges from each event back to its `source_object_id`.

#### 3.3 Aggregation

`query_timeline_aggregated` in `timeline_service.rs` performs server-side grouping:

```sql
SELECT event_type, description, COUNT(*) AS cnt,
       MIN(ts) AS first_ts, MAX(ts) AS last_ts,
       GROUP_CONCAT(id, ',') AS sample_ids
FROM timeline_events
GROUP BY event_type, description
ORDER BY cnt DESC, event_type ASC, description ASC
LIMIT ? OFFSET ?
```

The result is shaped into `TimelineStripeDto` per `event_type` with `TimelineClusterDto` items. This is a useful backend capability, but **no frontend page currently calls the aggregated endpoint**; the Timeline page only uses the flat paginated list.

### 4. sourceObjectId Correlation with Artifacts

`source_object_id` is the investigation bridge. The same UUID is used for:
- `file_entries.id` (the originating file);
- `timeline_events.source_object_id` (MACB and artifact-derived events);
- `artifacts.source_object_id` (the file the artifact was extracted from);
- `graph_nodes.id` for `NodeType::TimelineEvent` and `target_id` for `References` edges.

This is the primary mechanism by which the frontend lets an investigator jump from a timeline event to its source. In `Timeline.tsx`, the "跳转到来源对象" button:
- routes to `/artifacts` if `sourceObjectId` starts with `artifact:` (legacy artifact prefix handling);
- otherwise routes to `/files` and sets the selected file ID.

The correlation graph (Section 8) builds `References` edges between timeline events and their source files, enabling artifact-to-timeline leads. However, this relies entirely on extractors setting the field correctly. `AGENTS.md` Gotcha #15 explicitly warns: *"Every new artifact extractor must set this field or cross-artifact leads will silently miss connections."* The warning is not enforced at compile time or by a runtime validator today.

### 5. Persistence Schema

`crates/persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql`:

```sql
CREATE TABLE timeline_events (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL DEFAULT '',
    source_object_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    ts TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    attrs TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_timeline_case_ts ON timeline_events(case_id, ts);
CREATE INDEX idx_timeline_type ON timeline_events(event_type);
CREATE INDEX idx_timeline_source ON timeline_events(source_object_id);
```

Subsequent migrations add `parser_id`, `parser_version`, `confidence`, and `source_attribution` columns. The `ts` column is stored as RFC3339 text, which allows string-range comparisons. The `TimelineRepo` query order is `ts DESC, id ASC`, giving deterministic pagination for identical timestamps.

### 6. Frontend Timeline Page

`frontend/src/app/pages/Timeline.tsx` implements:

- **Histogram**: 60 buckets built from `Date.parse(e.ts)` over the current event set; heights are normalized to the max bucket count. It provides visual density but no click-to-zoom behavior.
- **Filters**: `datetime-local` inputs for start/end time, a `<select>` for event type populated from the current page's event types, and a clear button. ISO8601 normalization is done via `new Date(timeStart).toISOString()`.
- **Table**: `DenseDataTable` with columns timestamp, data source (`attrs.source`), event type, and title. Row selection is stored in `selection-store`.
- **Inspector pane**: shows timestamp, event type, description, source object, and the "Jump to source" action.
- **Hooks**: `useTimelineEvents` and `useTimelineEventById` in `frontend/src/features/timeline/hooks.ts` wrap TanStack Query; API calls route through `apiClient.request` in `frontend/src/lib/api/timeline.ts`.

Notable frontend limitations:
- The Zoom In/Zoom Out buttons are present but **not wired to any state change**.
- Granularity is always labeled "自适应" (adaptive) with no user control.
- The histogram can produce `NaN` buckets if server timestamps are malformed; the code has basic guards but no user-facing error feedback.
- Event type filter is built from the current page only, so it cannot show types that are not in the first 100 returned events.

### 7. Strengths

1. **Clear dual-path design**: filesystem MACB and artifact extractors both feed a single `timeline_events` table, giving a unified view of file, program, browser, and log activity.
2. **Idempotent MACB projection**: `timeline_projection_meta` and deterministic `macb:{file_id}:{event_type}` IDs prevent duplicate events on re-analysis.
3. **Parallel event generation**: `project_and_store_macb` uses Rayon (`par_iter().flat_map_iter`) for CPU-bound MACB projection from large file catalogs.
4. **Correlation-first model**: `source_object_id` ties file entries, timeline events, artifacts, and graph nodes together, enabling cross-artifact investigation.
5. **Typed service errors**: `TimelineServiceError` uses `thiserror` and maps to `ApiErrorDto` categories (`Db`, `NotFound`, `InvalidInput`, `Other`).
6. **Comprehensive backend tests**: `crates/timeline` tests cover MACB semantics, edge cases (epoch, future, deleted), and batch behavior; `timeline_service` tests cover idempotent projection, filtered queries, and aggregation performance up to 10,000 events.
7. **Server-side aggregation capability**: the grouped-by-description stripe API is ready for future UI features (density overview, heat map).

### 8. Risks and Issues Found

#### 8.1 MACB projection is filesystem-agnostic and can be semantically lossy

The `FILE_METADATA_CHANGED` event maps to the `changed_at` column from whatever file system parser produced the entry, but this column's meaning varies across NTFS, FAT, ext4, etc. There is no per-filesystem attribution to help investigators interpret `C` vs `B` semantics.

#### 8.2 `timeline` crate is a single-file module

At 585 lines it is still within the V3 1500-line limit, but as registry, EVTX, browser, and Recycle Bin projection logic grows, the crate will outgrow a single file. There is no `timeline/projections/` structure today.

#### 8.3 No central timeline event factory or validator

Extractors create `TimelineEvent` via `new_timeline_event` (artifact-core) or `make_timeline_event` (analysis service). Titles, descriptions, `confidence`, and `parser_version` are set independently. A malformed extractor could omit `source_object_id`, use an invalid `event_type`, or produce a non-RFC3339 timestamp, and the code does not validate before insertion.

#### 8.4 Graph population is non-fatal and silent

`populate_timeline_event_graph` is called with `let _ = ...` in `ensure_macb_timeline_projected`. If graph writes fail, the timeline still exists but the correlation graph is incomplete, with no user-visible warning.

#### 8.5 Frontend date handling is brittle

`Timeline.tsx` uses `new Date(timeStart).toISOString()` and `Date.parse(e.ts)` for filters and the histogram. Invalid input is silently ignored or produces NaN buckets. There is no `try/catch` around `new Date()` and no validation message for the user.

#### 8.6 Zoom/granularity controls are UI-only

The Zoom In/Zoom Out buttons and the "自适应" label do not actually change bucket count or query resolution. The histogram is always 60 buckets and the query limit is always 100.

#### 8.7 Event type filter is page-local

The `<select>` is populated only from events returned in the current 100-row page. Investigators cannot filter by an event type that happens to fall outside the most recent 100 events.

#### 8.8 Aggregation endpoint is not consumed

`query_timeline_aggregated` is implemented and tested, but no frontend command or page calls it, so the server-side grouping capability is dead code from the UI perspective.

### 9. Improvement Recommendations

#### P0 — Before V2 release

1. **Add a `source_object_id` enforcement test** for every artifact extractor. Create a test harness that runs each extractor against a representative fixture and asserts that at least one emitted timeline event or artifact carries a non-empty `source_object_id` equal to the input file ID. This operationalizes `AGENTS.md` Gotcha #15.
2. **Surface graph population failures** to the user. Change `ensure_macb_timeline_projected` to return a warning or emit a job event when `populate_timeline_event_graph` fails, so investigators know correlation edges may be incomplete.
3. **Validate frontend date inputs**. Replace silent `NaN` handling with explicit error messages and disable the Apply/Clear actions when the datetime-local value is invalid.
4. **Wire or remove zoom/granularity controls**. Either implement bucket-count adjustment (e.g., 30/60/120/240) or hide the buttons and the "自适应" label until the feature is implemented.

#### P1 — Near-term engineering debt

5. **Split the `timeline` crate** into `timeline/projections/{macb,artifact,registry,evtx,browser}.rs` and a shared `event.rs` for `TimelineEvent` construction helpers. Keep `project_file_macb` in the MACB module.
6. **Introduce a timeline event schema validator** used by both `TimelineRepo::insert_batch` and `flush_worker_rows`. Reject events with empty `source_object_id`, unknown `event_type`, or unparseable timestamps before insertion.
7. **Normalize event type taxonomy**. Define a `TimelineEventType` enum or constant registry for MACB (`FILE_CREATED`, `FILE_MODIFIED`, `FILE_ACCESSED`, `FILE_METADATA_CHANGED`) and artifact-derived types (`PROGRAM_EXECUTION`, `BROWSER_VISIT`, `BROWSER_DOWNLOAD`, `EVTX_EVENT`, etc.) to prevent typographic drift and enable reliable filtering.
8. **Add a per-filesystem source attribution** to MACB events. Extend `source_attribution` to indicate the origin column and file system parser (e.g., `"ntfs:$STANDARD_INFORMATION:modified_at"`), so investigators can interpret MACB semantics correctly.
9. **Consume the aggregation endpoint** in the frontend. Add a summary/overview mode to the Timeline page that uses `TimelineAggregatedDto` to show stripe counts and clusters before the investigator drills into the raw event list.

#### P2 — Hardening and polish

10. **Add a timeline density regression test** against a real E01 fixture with known MACB counts, so changes to filesystem parsers or the MACB projection do not silently alter event counts.
11. **Implement cursor/keyset pagination** for the event table. The current `OFFSET` pagination is fine for tens of thousands of rows but will degrade as cases grow to millions of events.
12. **Document the correlation bridge** for investigators and new developers: a short runbook explaining how `source_object_id` links file entries, timeline events, artifacts, and graph nodes, and what to check when leads are missing.
13. **Consider time-zone handling**. The frontend formats timestamps with `toLocaleString('zh-CN')`, which may surprise non-Chinese users. Allow locale configuration or use ISO UTC display consistently.

### 10. Code Pointers

| Concern | File |
|---|---|
| MACB projection | `crates/timeline/src/lib.rs` |
| Service orchestration | `crates/app-services/src/timeline_service.rs` |
| DTO definitions | `crates/transport/src/dto/timeline.rs` |
| Domain entity | `crates/domain/src/timeline/mod.rs` |
| SQLite persistence | `crates/persistence-sqlite/src/repositories/timeline_repo.rs` |
| Schema | `crates/persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql` |
| Artifact sink interface | `crates/artifacts-core/src/lib.rs` |
| Prefetch timeline events | `crates/artifacts-windows/src/prefetch/parser.rs` |
| LNK timeline events | `crates/artifacts-windows/src/lnk/parser.rs` |
| Browser timeline events | `crates/app-services/src/analysis_service/extraction/browser.rs` |
| Analysis extraction orchestrator | `crates/app-services/src/analysis_service/extraction/mod.rs` |
| Import worker timeline staging | `crates/app-services/src/import_analysis/worker_runtime.rs` |
| Frontend page | `frontend/src/app/pages/Timeline.tsx` |
| Frontend hooks | `frontend/src/features/timeline/hooks.ts` |
| Frontend API | `frontend/src/lib/api/timeline.ts` |
| Frontend types | `frontend/src/types/timeline.ts` |

---

## 9. Deep Dive — Forensic Overview

### Summary

- **Artifact extraction is plugin-driven**. The `artifacts-core` crate defines `ArtifactExtractor` and `ArtifactSink` traits; `artifacts-windows` implements extractors for browser history, EVTX, jump lists, LNK, prefetch, recycle bin, registry, SRU, and thumbcache.
- **Analysis orchestration lives in `app-services`**. `analysis_service::extraction::run_analysis_extraction` pre-loads registry hives and transaction logs, then dispatches candidates to specialized extractors (browser, email, EVTX, registry) and writes artifacts + timeline events to SQLite.
- **The forensic overview UI is built from three pages**: `CaseOverview` (case metrics and recent tasks), `V3Dashboard` (graph stats, governance snapshot, correlation), and `V3ScoreCards` (shared stat cards).
- **Artifact browsing is family-based**. The `Artifacts` page lists artifact families as tabs and uses `DenseDataTable` for rows; selecting an artifact updates the global selection store so other panes can correlate.
- **Coverage is Windows-primary, with non-Windows crates unintegrated**. `artifacts-linux`, `artifacts-macos`, and the mobile crates (`artifacts-ios`, `artifacts-android`) have parser modules but are not dispatched by `analysis_service::extraction`, so their artifacts do not yet reach the case database or timeline.

### Key Components

| Component | Role | Key file(s) |
|---|---|---|
| `ArtifactExtractor` trait | Plugin interface for extractors | `crates/artifacts-core/src/lib.rs` |
| `artifacts-windows` extractors | Windows-specific artifact parsers | `crates/artifacts-windows/src/{browser,evtx,lnk,prefetch,registry,recycle_bin,...}` |
| `analysis_service` | Orchestrates extraction pipeline | `crates/app-services/src/analysis_service/extraction/mod.rs` |
| `ArtifactSink` / `VecSink` | Collects artifacts and timeline events | `crates/artifacts-core/src/lib.rs` |
| `containers-pst` | PST/OST/MBOX email container parsers | `crates/containers-pst/src/{pst,ost,mbox}.rs` |
| `artifacts-linux` | Linux artifact parsers (journal, wtmp, bash, apt, cron, sudo) | `crates/artifacts-linux/src/lib.rs` |
| `artifacts-macos` | macOS artifact parsers (FSEvents, LaunchServices, plist, quarantine, etc.) | `crates/artifacts-macos/src/lib.rs` |
| `artifacts-ios` / `artifacts-android` | Mobile artifact parser stubs | `crates/artifacts-ios/src/lib.rs`, `crates/artifacts-android/src/lib.rs` |
| `CaseOverview` | Case metrics + recent tasks | `frontend/src/app/pages/CaseOverview.tsx` |
| `V3Dashboard` | Graph + governance + correlation overview | `frontend/src/app/pages/V3Dashboard.tsx` |
| `Artifacts` | Family-based artifact browser | `frontend/src/app/pages/Artifacts.tsx` |

### How Extraction Works

1. **Candidate discovery**. `evidence_candidates_for_categories` scans the file catalog for paths matching known artifact patterns (e.g., `NTUSER.DAT`, `*.evtx`, `History` SQLite files).
2. **Registry pre-load**. For registry candidates, the extractor reads the full hive (up to `MAX_ANALYSIS_SOURCE_BYTES` = 128 MiB), pre-computes BootKeys from `SYSTEM`, and loads `.LOG1`/`.LOG2` transaction logs.
3. **Extractor dispatch**. Each candidate is matched to extractors by `supports_path`; extractors receive an `ArtifactContext` (file reader + path + ID) and emit artifacts/timeline events via a sink.
4. **Persistence**. Artifacts are written to `artifacts` table with `source_object_id` set to the originating file entry; timeline events are written to `timeline_events`.
5. **Graph population**. After extraction, correlation logic builds graph nodes and edges linking artifacts, timeline events, and files.

### Platform and Container Coverage

| Platform / Container | Crate | Status | Notes |
|---|---|---|---|
| Windows | `artifacts-windows` | Mature | Registry hives (SYSTEM/SOFTWARE/SAM/SECURITY/NTUSER/USRCLASS/Amcache), EVTX (7 channels), Browser, Prefetch, LNK, JumpList, RecycleBin, SRU, Thumbcache |
| Linux | `artifacts-linux` | Partial | systemd journal, `wtmp` login records, bash history, apt/dpkg history, crontab, sudo auth log |
| macOS | `artifacts-macos` | Partial | FSEvents, Launch Services, binary/XML plist, quarantine events, recent items, Spotlight, unified log tracev3 |
| PST / OST / MBOX | `containers-pst` | Integrated | `PstReader`, `OstReader`, and `mbox::parse_mbox` are wired into `analysis_service::extraction::email.rs` |
| iOS / Android | `artifacts-ios` / `artifacts-android` | Placeholders | Modules exist (iOS: backup, calls, contacts, messages, notes, photos, safari; Android: backup, calls, chrome, contacts, sms) but are not integrated into the main `analysis_service` extraction pipeline |

### Highlighted Strengths

1. **Trait-based extractor registry**. New Windows artifact extractors can be added without modifying the core analysis loop; `ExtractorRegistry` discovers them by path pattern.
2. **Registry transaction-log recovery**. The extractor loads `.LOG1`/`.LOG2` and applies them via `txlog.rs` before parsing, improving completeness of volatile registry data.
3. **BootKey caching**. SAM/SECURITY hash decryption reuses the pre-computed BootKey from the same data source rather than re-reading `SYSTEM` for every hive.
4. **Frontend overview is data-driven**. `V3Dashboard` composes five TanStack Query hooks (`useCurrentCase`, `useGraphSnapshot`, `useTimelineEvents`, `useArtifactFamilyCounts`, `useCorrelationSnapshot`, `useV3GovernanceSnapshot`) to build a unified dashboard.
5. **Selection store enables cross-pane correlation**. Clicking a file, artifact, timeline event, or search hit updates `selection-store.ts`; other panes react to the same ID.

### Risks and Issues Found

#### 1. Windows artifact maturity dominates the platform claim

The project is described as a cross-platform forensic workbench, but `artifacts-windows` is substantially more mature than `artifacts-linux`, `artifacts-macos`, and mobile crates. The V3/V4 phases added Linux/macOS crates, but real-sample coverage and extractor counts are likely far behind Windows.

#### 2. `AnalysisServiceError` typed errors vs extractor `String` returns

The `ArtifactExtractor::run` trait returns `Result<ExtractorReport, String>`. Extractors that hit parsing errors must encode them as strings, losing the typed error taxonomy used elsewhere. This is a known debt carried over from the `Result<String, String>` cleanup.

#### 3. Non-Windows artifact crates are not wired into the main extraction pipeline

`analysis_service::extraction` only dispatches the four Windows-centric categories (`Registry`, `BrowserHistory`, `Email`, `EventLogs`). The `artifacts-linux`, `artifacts-macos`, `artifacts-ios`, and `artifacts-android` crates have parser modules but are not invoked by `run_analysis_extraction`, so their output never reaches the database or timeline on a real case.

#### 4. No artifact extraction progress granularity

The import pipeline emits coarse progress events, but artifact extraction is largely opaque to the frontend once it starts. A long registry parse can leave the investigator waiting without per-extractor progress.

#### 5. Overview pages fetch independently

`V3Dashboard` fires six parallel queries but has no single snapshot command. If one query fails, the dashboard shows partial data with an error banner. A single backend "overview snapshot" command would reduce round-trips and ensure consistency.

### Improvement Recommendations

#### P0 — Before V2 release

1. **Wire Linux and macOS artifact families into the extraction pipeline** by adding `LinuxArtifacts` and `MacArtifacts` categories to `analysis_service::extraction`, with candidate discovery and per-family extractor dispatch.
2. **Add a real Windows E01 regression test for artifact families** beyond the 7 existing E01 tests, to ensure at least one artifact per major family (Browser, EVTX, Registry, LNK, Prefetch) is extracted and counted.

#### P1 — Near-term engineering debt

3. **Change `ArtifactExtractor::run` to return a typed error trait** or per-extractor error enum, instead of `String`, so extraction failures can be classified and surfaced correctly.
4. **Add per-extractor progress events** (e.g., `registry:extract:progress`) so the frontend can show granular status during long analysis runs.
5. **Create a single backend overview snapshot command** that returns case metrics, graph counts, artifact family counts, and governance status in one request.

#### P2 — Hardening and polish

6. **Build a cross-platform artifact roadmap**. Document which artifact families are supported on Windows vs Linux vs macOS, and prioritize real-sample validation for Linux/macOS to match the Windows level.
7. **Add artifact extractor regression tests** with small fixture files (e.g., a synthetic `NTUSER.DAT`, a single EVTX) so parser changes can be validated without mounting full disk images.

---

## 10. Consolidated Risk Register and Recommendations

### 10.1 Risk register

The following table consolidates the most significant risks found across all audit areas. Severity is a composite of business impact and exploitability/technical exposure; likelihood reflects current exposure on the codebase.

| ID | Risk | Severity | Likelihood | Area | Cross-reference |
|---|---|---|---|---|---|
| R1 | Manual Rust/TypeScript DTO and event synchronization causes runtime drift without CI detection | High | Medium | Architecture | §6.5, §6.6 |
| R2 | Non-Windows artifact crates (`artifacts-linux`, `artifacts-macos`, mobile) are not wired into the extraction pipeline, making the cross-platform claim incomplete | High | High | Forensic overview | §9.3 |
| R3 | Residual `Result<String, String>` error returns bypass typed error taxonomy and UI classification | Medium | Medium | Backend quality | §4.3.1 |
| R4 | Production `unwrap()`/`expect()` calls (68/38) can panic on attacker-controlled evidence | Medium | High | Backend quality | §4.3.5 |
| R5 | Five modules exceed the 1500-line policy, hurting maintainability and testability | Medium | High | Backend quality | §4.3.2 |
| R6 | Dynamic SQL composition via `format!` in six repositories is low-risk today but a future SQL-injection regression vector | Medium | Low | Backend quality | §4.3.6 |
| R7 | Error classification by substring matching is brittle and can misclassify new errors | Medium | Medium | Architecture / Backend quality | §4.3.7, §6.5.4 |
| R8 | Search indexing is coupled to artifact analysis; users cannot search immediately after import | Medium | Medium | Search | §7.3.1 |
| R9 | Search UI placeholder advertises SQL-like syntax that the Tantivy backend does not support | Medium | Medium | Search / Frontend | §7.3.4 |
| R10 | `catalog` crate is deprecated dead code with no consumers | Low | High | Search | §7.3.2 |
| R11 | Timeline zoom/granularity controls are UI-only and do not affect query behavior | Medium | Medium | Timeline | §8.3.6 |
| R12 | `source_object_id` correlation is not enforced by tests or validators; missing values silently break cross-artifact leads | High | Medium | Timeline / Forensic overview | §8.3.3, §9.2 |
| R13 | Frontend `Timeline` date parsing can produce `NaN` buckets without user feedback | Low | Medium | Timeline / Frontend | §8.3.5 |
| R14 | `V3Dashboard` is near the 500-line component limit and mixes concerns | Low | High | Frontend quality | §5.2 |
| R15 | Low per-file test coverage in `CaseActions.tsx` and GQL notebook components hides regressions | Low | Medium | Frontend quality | §5.4 |
| R16 | `mcp-store.ts` bundles transport, protocol, and UI state in one file | Low | Medium | Frontend quality | §5.3 |
| R17 | `deny.toml` advisory exceptions expire 2026-09-01 and may require a Tauri upgrade | Medium | Medium | Backend quality | §4.3.3 |
| R18 | AppState mixes case, task, MCP, and cache concerns | Low | Medium | Architecture | §6.5.3 |
| R19 | Long-running tasks run on the Tauri Tokio runtime and can starve the UI command loop | Medium | Low | Architecture | §6.5.5 |

### 10.2 Consolidated improvement recommendations

The following lists combine all P0/P1/P2 recommendations from the preceding sections, grouped by priority so the project can build a single backlog.

#### P0 — Remediate before V2 release

| # | Recommendation | Owner | Area | Status |
|---|---|---|---|---|
| P0.1 | Add an IPC contract regression test / drift-detection guard for Rust DTOs and TypeScript interfaces | Architecture | §6.7.1, §6.7.3 | Not implemented |
| P0.2 | Wire Linux and macOS artifact families into `analysis_service::extraction` with candidate discovery | Forensic overview | §9.5.1 | Not implemented |
| P0.3 | Add a real Windows E01 regression test covering at least Browser, EVTX, Registry, LNK, and Prefetch artifact families | Forensic overview | §9.5.2 | Not implemented |
| P0.4 | Convert the 6 residual `Result<String, String>` functions to typed errors | Backend quality | §4.4.1 | Not implemented |
| P0.5 | Split `crates/app-services/src/file_service/viewer.rs` (3,044 lines) into focused submodules | Backend quality | §4.4.2 | Not implemented |
| P0.6 | Split `crates/fs-ntfs/src/lib.rs` (2,026 lines) into MFT/data-runs/attribute/reader modules | Backend quality | §4.4.3 | Not implemented |
| P0.7 | Clarify the search query language in the UI; replace the SQL-like placeholder | Search | §7.4.1 | Not implemented |
| P0.8 | Remove or formally deprecate the `catalog` crate from the workspace | Search | §7.4.2 | Not implemented |
| P0.9 | Document full-text search coverage limits in the UI and manual | Search | §7.4.3 | Not implemented |
| P0.10 | Add a `source_object_id` enforcement test for every artifact extractor | Timeline / Forensic overview | §8.4.1 | Not implemented |
| P0.11 | Surface timeline graph-population failures to the user | Timeline | §8.4.2 | Not implemented |
| P0.12 | Validate frontend `Timeline` date inputs and disable actions on invalid input | Timeline / Frontend | §8.4.3 | Not implemented |
| P0.13 | Wire or remove the timeline zoom/granularity controls | Timeline / Frontend | §8.4.4 | Not implemented |
| P0.14 | Split `frontend/src/app/pages/V3Dashboard.tsx` before it crosses 500 lines | Frontend quality | §5.4 | Not implemented |
| P0.15 | Add coverage for `CaseActions.tsx` and GQL notebook components | Frontend quality | §5.4 | Not implemented |

#### P1 — Near-term engineering debt (next 1–2 sprints)

| # | Recommendation | Area |
|---|---|---|
| P1.1 | Split remaining oversized files: `email.rs`, `fs-apfs/src/lib.rs`, `evtx/parser.rs` | Backend quality |
| P1.2 | Centralize the 6 non-vendored direct version dependencies in workspace root | Backend quality |
| P1.3 | Reduce `unwrap`/`expect` density in project-owned code (`fact_loader.rs`, `fs-apfs/checkpoint.rs`, etc.) | Backend quality |
| P1.4 | Introduce a repository SQL helper for dynamic `IN`/placeholder queries and conditional `WHERE` clauses | Backend quality |
| P1.5 | Replace substring-based error classification with explicit `category()` methods or `From` implementations | Backend quality / Architecture |
| P1.6 | Refresh `deny.toml` advisory exceptions before 2026-09-01 or upgrade Tauri | Backend quality |
| P1.7 | Decouple full-text indexing from artifact analysis with a post-import indexing step | Search |
| P1.8 | Add index health/rebuild API | Search |
| P1.9 | Expand text extraction coverage (PDF/Office) or remove the misleading `.doc`/`.xls` placeholder | Search |
| P1.10 | Split `timeline` crate into projection modules | Timeline |
| P1.11 | Add timeline event schema validator rejecting empty `source_object_id`, unknown `event_type`, and invalid timestamps | Timeline |
| P1.12 | Normalize `TimelineEventType` taxonomy to prevent string drift | Timeline |
| P1.13 | Add per-filesystem source attribution to MACB events | Timeline |
| P1.14 | Consume the timeline aggregation endpoint in the frontend | Timeline |
| P1.15 | Refactor `frontend/src/app/pages/use-file-browser.ts` (464 lines) into focused hooks | Frontend quality |
| P1.16 | Refactor `frontend/src/stores/mcp-store.ts` into smaller stores | Frontend quality |
| P1.17 | Raise Vitest coverage thresholds from 45/45/45/35 to 55/55/55/45 | Frontend quality |
| P1.18 | Standardize Tailwind color usage by replacing inline hex literals with theme tokens | Frontend quality |
| P1.19 | Refactor `AppState` into focused sub-states | Architecture |
| P1.20 | Add command audit logging for case create/delete, file extract, data-source delete, MCP tool calls | Architecture |
| P1.21 | Freeze the 18 event topics and document payloads in `docs/ipc-event-contract.md` | Architecture |
| P1.22 | Change `ArtifactExtractor::run` to return a typed error trait/enum | Forensic overview |
| P1.23 | Add per-extractor progress events | Forensic overview |
| P1.24 | Create a single backend overview snapshot command | Forensic overview |

#### P2 — Hardening and polish

| # | Recommendation | Area |
|---|---|---|
| P2.1 | Add a module-size lint/fail to CI | Backend quality |
| P2.2 | Re-enable `cargo clippy --workspace --all-targets -- -D warnings` as a pre-merge gate | Backend quality |
| P2.3 | Lint against new `format!` SQL once the SQL helper is in place | Backend quality |
| P2.4 | Add search result regression tests with a fixture index | Search |
| P2.5 | Consider moving the Tantivy index into a case-cache sub-directory | Search |
| P2.6 | Evaluate replacing the custom highlighter with Tantivy's snippet generator | Search |
| P2.7 | Make the 1000-result search pagination cap configurable | Search |
| P2.8 | Add a timeline density regression test against a real E01 fixture | Timeline |
| P2.9 | Implement cursor/keyset pagination for large timeline event tables | Timeline |
| P2.10 | Document the `source_object_id` correlation bridge runbook | Timeline |
| P2.11 | Add locale/time-zone configuration for timestamp display | Timeline |
| P2.12 | Build a cross-platform artifact roadmap and real-sample validation plan | Forensic overview |
| P2.13 | Add small-fixture regression tests for individual artifact extractors | Forensic overview |
| P2.14 | Create shared test fixtures / page objects for oversized page tests | Frontend quality |
| P2.15 | Add an ESLint warning for production `.tsx` files approaching 400 lines | Frontend quality |
| P2.16 | Document frontend state-management conventions | Frontend quality |
| P2.17 | Consider a component-story or visual-regression harness for low-covered UI | Frontend quality |
| P2.18 | Evaluate `ts-rs` / `typeshare` for DTO generation, or document the no-codegen policy | Architecture |
| P2.19 | Document and test media handle invalidation on case close | Architecture |
| P2.20 | Consider isolating long-running tasks on a dedicated worker pool/process | Architecture |

---

## 11. Appendix: Audit Evidence

### Files generated during this audit

| File | Purpose |
|---|---|
| `docs/compose/specs/2026-06-30-forensics-complete-audit-design.md` | Audit design/specification |
| `docs/compose/plans/2026-06-30-forensics-complete-audit-plan.md` | Audit execution plan |
| `docs/compose/reports/audit-raw-notes.md` | Raw gate results and project metadata |
| `docs/compose/reports/section-backend-quality.md` | Backend quality section draft |
| `docs/compose/reports/section-frontend-quality.md` | Frontend quality section draft |
| `docs/compose/reports/section-architecture.md` | Architecture section draft |
| `docs/compose/reports/section-search-catalog.md` | Search/catalog deep-dive section draft |
| `docs/compose/reports/section-timeline.md` | Timeline deep-dive section draft |
| `docs/compose/reports/section-forensic-overview.md` | Forensic overview deep-dive section draft |
| `docs/compose/reports/2026-06-30-forensics-audit-report.md` | This integrated report |

### Guard script and gate results

All of the following commands were executed during the audit and produced passing results on 2026-06-30:

| Command | Result | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | No formatting diff |
| `pnpm --dir frontend typecheck` | PASS | `tsc --noEmit` |
| `pnpm --dir frontend lint` | PASS | ESLint on `src/` |
| `powershell -ExecutionPolicy Bypass -File scripts/check-command-sql-boundary.ps1` | PASS | No raw SQL in production command handlers |
| `powershell -ExecutionPolicy Bypass -File scripts/check-dead-code-allow-guard.ps1` | PASS | No `#[allow(dead_code)]` in production |
| `powershell -ExecutionPolicy Bypass -File scripts/check-media-protocol-guard.ps1` | PASS | Media preview stays on `evidence-media:` |
| `powershell -ExecutionPolicy Bypass -File scripts/check-frontend-lockfile-policy.ps1` | PASS | Lockfile policy compliant |

### Coverage data

Frontend coverage was measured with `pnpm --dir frontend test:coverage`:

| Dimension | Threshold | Actual |
|---|---|---|
| Lines | 45% | 65.00% |
| Statements | 45% | 64.37% |
| Functions | 45% | 60.36% |
| Branches | 35% | 55.61% |

### Amendment A — Post-Audit Verification (2026-07-01)

This amendment records the results of a follow-up code review performed on 2026-07-01 against recent changes and the accuracy of the 2026-06-30 audit report.

#### Corrections applied to the report

- **Workspace crate count:** corrected from 37 to 38 crates (37 library crates + Tauri shell). The additional library crate was present in `Cargo.toml` but omitted from the original count.
- **EventTopic count:** corrected from 19 to 18 string constants / topics in `crates/transport/src/events/mod.rs`.
- **Frontend page count:** clarified that `frontend/src/app/pages/` contains 23 non-test `.tsx` files (17 top-level routed pages + 6 `settings/` sub-pages).
- **`case_service.rs` retry loop:** verified that the retry loop now terminates with `.expect("last_err must be Some after retry loop")` after 5 attempts, replacing the earlier `unwrap_or_else` pattern.

#### Verified recent optimization changes

The following recent optimizations were confirmed in source control:

- `hex-offset-parser` added with 24 test cases / 41 assertions (not 35 as claimed in an internal optimization note).
- `hex-range-merger` optimizations merged.
- Empty-file guards added to `frontend/src/features/files/hooks.ts` and related hooks.
- `case_service.rs` retry loop updated to use `expect` after the maximum retry count.
- `case_commands.rs` drain helper introduced.
- Cache invalidation fixes in `frontend/src/features/analysis/hooks.ts` and `frontend/src/features/graph/hooks.ts`.

#### Confirmed accurate claims

The following claims in the original audit remain accurate after verification:

- 96 Tauri commands registered in `apps/desktop/src-tauri/src/lib.rs`.
- 6 residual `Result<String, String>` functions in production code.
- Five production modules exceed the 1500-line policy limit.
- 68 production `.unwrap()` calls and 38 production `.expect()` calls (outside `#[cfg(test)]`).
- Six non-vendored direct version dependencies outside the workspace root.
- `V3Dashboard.tsx` remains the largest page at 471 lines (under the 500-line limit).
- 76 frontend test files.
- Zero direct `invoke` calls outside `frontend/src/lib/api/`.
- Frontend coverage thresholds remain 45/45/45/35 and current actuals exceed them.
- Media preview remains on the `evidence-media:` protocol and the guard script passes.
- `CommandError::from_service_error` still classifies errors by substring matching.

#### Stale or inaccurate claims corrected

In addition to the count corrections above, the report previously implied the frontend page directory contained only 17 `.tsx` files without distinguishing routed pages from settings sub-pages. The settings sub-pages (`settings/`) account for the difference and are now enumerated.

#### Recommendation

Re-run the full PowerShell guard suite (`scripts/check-*.ps1`) and the Rust/frontend test suites in a Visual Studio 2022 developer environment before finalizing the next release, to ensure the recent optimizations and report corrections have not introduced regressions.

### Limitations of this audit

- The full Rust test and clippy suites were not re-run because they require a Visual Studio 2022 developer environment with `vcvars64.bat`.
- The audit is static and based on source inspection; no runtime profiling or dynamic testing was performed.
- Counts and file locations are accurate as of the audit date; active development may change line numbers and counts after the report is produced.

---

*End of report.*








