# Section 3: Backend Code Quality Analysis

## Summary of Backend Quality Posture

- The backend is **strongly typed and well-layered**: the majority of crates use domain-specific `thiserror` enums, and Tauri commands delegate to `app-services` rather than mixing business logic or SQL. Six residual `Result<String, String>` functions remain in `exchange` and `artifacts-windows` registry code as exceptions.
- **The quick-gate subset run during this audit passed**: `cargo fmt`, `pnpm typecheck`, `pnpm lint`, command/SQL boundary, dead-code, media-protocol, and frontend lockfile guards all passed on the audit date. The full `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` gates were not re-run as part of this audit.
- A few **legacy untyped error surfaces** remain (`Result<String, String>` in 6 places), mostly in registry/Windows artifact parsers and the STIX serializer.
- **Module size discipline has slipped**: five production `.rs` files exceed the 1500-line project ceiling, with one file (`file_service/viewer.rs`) at >3,000 lines.
- **Unsafe usage is minimal and documented**: only four production `unsafe` blocks exist, and all carry `// SAFETY:` comments; no dead-code suppressions remain in production code.
- **Workspace dependency centralization has minor drift**: six direct version dependencies exist outside the workspace root, all in non-vendored crates; the vendored `evtx-patched` crate is intentionally exempt.
- **Unwrap/expect density is a latent runtime risk**: 68 production `.unwrap()` calls and 38 production `.expect()` calls remain outside `#[cfg(test)]` blocks, with the highest concentration in the vendored EVTX parser.

## Quality Metrics

| Metric | Count | Notes / Source |
|---|---|---|
| Workspace crates | 37 | 36 library crates + Tauri shell (`apps/desktop/src-tauri`) |
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

## Highlighted Strengths

1. **Typed error taxonomy in production**. `crates/transport/src/errors.rs` defines a single `ApiErrorDto` plus a `CommandError` with forensic-aware categories (`validation`, `parser`, `security`, `external`, `timeout`, etc.). Service crates such as `case_service.rs` use `thiserror` enums (`CaseServiceError`) and never return `Result<T, String>`.
2. **Clean command/service/SQL boundary**. `case_commands.rs` and `file_commands.rs` contain no production raw SQL strings; all persistence logic lives in repository modules (`case_repo.rs`, `file_repo.rs`). The only raw SQL in `commands/` is inside `#[cfg(test)]` benchmark fixtures in `apps/desktop/src-tauri/src/commands/benchmarks.rs:169,174`, which the SQL-boundary guard intentionally exempts. The guard script `scripts/check-command-sql-boundary.ps1` passed on production paths.
3. **Dead-code policy is enforced**. `#[allow(dead_code)]` appears only in test files (`app-services/tests/registry_fixture_expected_test.rs`).
4. **Unsafe usage is minimal and audited**. The only production `unsafe` blocks are in Windows Compression API calls (`artifacts-windows/src/prefetch/parser.rs`) and Windows process-memory accounting (`app-services/src/import_analysis/progress.rs`). Each block has a `// SAFETY:` comment explaining why the call is sound.
5. **Dependency governance is explicit**. `deny.toml` requires `owner`, `reason`, and `expires` for every exception and disallows unknown registries or git sources. All 16 advisory exceptions are documented and currently unexpired.

## Risks and Issues Found

### 1. Residual untyped error returns (`Result<String, String>`)

Six occurrences remain in production code. These bypass the typed error taxonomy and make error classification harder in the UI.

- `crates/exchange/src/stix.rs:317`
- `crates/artifacts-windows/src/registry/txlog.rs:405`
- `crates/artifacts-windows/src/registry/parser.rs:62`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:3`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:11`
- `crates/artifacts-windows/src/registry/lookup/utf16.rs:23`

### 2. Oversized production modules (>1500 lines)

The project ceiling is 1500 lines per production source file. Five files exceed it:

| File | Lines | Risk |
|---|---|---|
| `crates/app-services/src/file_service/viewer.rs` | 3,044 | Preview/E01 reader cache, file handle resolution, and range reading are all in one file; hard to unit-test in isolation. |
| `crates/fs-ntfs/src/lib.rs` | 2,026 | NTFS reader, data runs, MFT parsing, and path resolution are tightly coupled in one module. |
| `crates/app-services/src/analysis_service/extraction/email.rs` | 1,903 | EML/MBox/PST extraction logic is oversized; should be split into `eml.rs`, `mbox.rs`, `pst.rs`. |
| `crates/fs-apfs/src/lib.rs` | 1,523 | Filesystem parser exceeds the limit. |
| `crates/artifacts-windows/src/evtx/parser.rs` | 1,505 | EVTX parser is at the boundary but should be split by channel/event type. |

### 3. Dependency advisory exception concentration

`deny.toml` carries 16 advisory exceptions, all expiring on `2026-09-01`. They are all transitive issues pulled in by Tauri (gtk3 bindings) or `urlpattern`/`tauri-utils`. If upstream has not released fixes by the expiry date, the project will either need to renew exceptions or absorb a breaking Tauri upgrade.

### 4. Workspace dependency centralization drift

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

### 5. Unwrap/expect density in production code

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

### 6. Dynamic SQL composition in repositories

Dynamic SQL is not limited to `file_repo.rs`. Several repositories build statements with `format!` for column lists, `IN (...)` placeholders, or conditional `WHERE` clauses:

- `file_repo.rs`: `format!("SELECT {FILE_ENTRY_COLUMNS} ...")` and dynamic `IN ({})` placeholders.
- `graph_repo.rs`: dynamic `IN` filter for edge types in `build_neighbor_query`, plus `format!` for column lists.
- `timeline_repo.rs`: `format!` for `WHERE 1=1` conditional clauses and LIMIT/OFFSET placeholders.
- `notebook_repo.rs`: `format!` for partial UPDATE `SET` clauses, LIKE filters, recursive CTE columns, and `IN` placeholders for batch citations.
- `batch_repo.rs`: `format!` for conditional `UPDATE ... SET status = ?1{now}` clauses.
- `audit_repo.rs`: `format!` for conditional `WHERE 1=1` case/action filters and LIMIT/OFFSET placeholders.

In every case the user-facing values are parameterized, so the current risk is low. However, the pattern is repeated across six repositories, and future changes could accidentally concatenate user input into a `format!` string. The project should centralize a single helper for `IN` placeholders, conditional `WHERE` clauses, and column-list expansion.

### 7. Error classification relies on substring matching

`CommandError::from_service_error` in `crates/transport/src/errors.rs` maps service errors to categories by scanning the lower-cased message for substrings such as `"timeout"`, `"parse"`, `"not supported"`, etc. This is brittle and can misclassify new error variants that happen to contain those words.

## Improvement Recommendations

### P0 — Remediate before next release

1. **Convert remaining `Result<String, String>` functions to typed errors.** Create small error enums in `exchange` and `artifacts-windows` (e.g., `RegistryError`, `StixError`) and return those instead. This completes the typed-error migration started in the V5 audit.
2. **Split `crates/app-services/src/file_service/viewer.rs`.** Decompose it into at least:
   - `viewer/handle.rs` — file handle creation and cache-key logic
   - `viewer/range.rs` — range reads and byte streaming
   - `viewer/e01_cache.rs` — per-case E01 reader cache
   - `viewer/preview.rs` — preview descriptor logic
3. **Split `crates/fs-ntfs/src/lib.rs`.** Separate `mft.rs`, `data_runs.rs`, `attribute.rs`, and `reader.rs` to make the NTFS parser maintainable and testable.

### P1 — Near-term engineering debt

4. **Split the remaining oversized files**: `email.rs`, `fs-apfs/src/lib.rs`, and `evtx/parser.rs`.
5. **Refresh dependency exceptions before 2026-09-01.** Evaluate whether Tauri 2.x or `urlpattern` updates are available; if not, extend the exceptions with a fresh technical review and updated expiry dates.
6. **Centralize the six non-vendored direct version dependencies in the workspace root.** Move `unicode-normalization`, `flate2`, `async-trait`, `tantivy`, `tauri-plugin-updater`, and `tauri-plugin-dialog` to the root `[workspace.dependencies]` table and reference them with `{ workspace = true }`. Keep `evtx-patched` as the documented exemption.
7. **Reduce unwrap/expect density in project-owned code.** Prioritize `fact_loader.rs` and `fs-apfs/src/checkpoint.rs`, then audit the remaining project-owned files. Replace each call with `?` and a typed error where the failure is recoverable, or use `unwrap_or`/`unwrap_or_default` for truly optional values. Leave the vendored `evtx-patched` reductions as a separate, longer-term fork-maintenance task.
8. **Introduce a repository SQL helper for dynamic IN/placeholder queries.** A helper that accepts `&[&str]` and returns a parameterized statement would remove the repeated `format!` blocks across `file_repo.rs`, `graph_repo.rs`, `timeline_repo.rs`, `notebook_repo.rs`, `batch_repo.rs`, and `audit_repo.rs`.

### P2 — Hardening and polish

9. **Replace substring-based error classification.** Move error classification to the typed error enum level: add a `category()` method to service errors or implement `From<CaseServiceError>` for `CommandError` so the mapping is explicit and compiler-checked.
10. **Add a module-size lint/fail to CI.** The project already has a 1500-line policy; add a lightweight check (e.g., `scripts/check-module-size.ps1`) to prevent further regressions.
11. **Consider running `cargo clippy --workspace --all-targets -- -D warnings` as a pre-merge gate.** While the project documents this as a default gate, the audit did not re-run it; ensure it passes on the current codebase to catch new lint regressions.
12. **Expand the SQL helper to all repositories and add a lint against new `format!` SQL.** Once the helper is in place, enforce it through a repository guard script to keep the SQL-composition surface from growing.

(End of file - total 162 lines)
