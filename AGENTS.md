# AGENTS.md

## Project Overview

**Meow~Detective** (`Meow_Detective` in source-level identifiers and storage names) is a Windows-first, single-user desktop digital-forensics application built with **Tauri 2**. It is backend-led: 36 Rust crates perform evidence processing (disk images, volume detection, file systems, Windows/Linux artifacts, search indexing, timeline generation, entity resolution, STIX 2.1 exchange), while a **React 18 + TypeScript + Vite + Tailwind 4** frontend provides the investigator UI.

- **Runtime**: Tauri 2 desktop shell. No HTTP server. All frontend↔backend communication goes through Tauri commands and events.
- **Primary platform**: `x86_64-pc-windows-msvc` (Windows-primary, desktop-first, single-user).
- **Storage**: SQLite case databases with WAL, migrations, and repository layer (24 repos, 49 migration scripts).
- **Evidence access**: Read-only. Original evidence sources are never modified.
- **Current status**:
  - V2: ~90% complete, Grade B (81/100), all 7 real E01 regression tests passing.
  - V3: ~89% complete; the retained production slice includes PST/OST/mbox (`containers-pst`) and Linux artifacts (`artifacts-linux`).
  - V4: Retained core delivered — 3 Linux filesystem crates (`fs-ext4`, `fs-xfs`, `fs-btrfs`) and the `exchange` crate (entity resolution, STIX 2.1, Ed25519 signing, chain-of-custody).
  - V5: Quality audit complete — architecture compliance 97%, runtime safety 96%, forensic completeness 96%. E01 preview pipeline hardened with partition path prefix, inode-based file resolution, and per-partition chunk-table caching.
- **Production analysis platforms**: Windows and Linux only. macOS requests and legacy macOS cases return typed `Unsupported`. APFS/HFS+ partition metadata may be recognized, but no filesystem reader or artifact extractor is available.

## Build and Test Commands

### Rust backend

```bash
# Format (no linker required)
cargo fmt --all

# Format check
cargo fmt --all -- --check

# Lint — requires Visual Studio vcvars64 on Windows
cargo clippy --workspace --all-targets -- -D warnings

# Test all Rust crates
cargo test --workspace

# Test one crate
cargo test -p <crate-name>

# Build the Tauri shell crate
cargo build -p forensics-desktop
cargo check -p forensics-desktop
```

> **Windows linker note**: Any command that links (`test`, `build`, `clippy --all-targets`) must run inside a Visual Studio 2022 developer environment. If plain bash fails to find `kernel32.lib` or resolves `link.exe` to Git's copy, run through `vcvars64.bat` first. See `CLAUDE.md` for a one-line PowerShell invocation that calls `vcvars64.bat` and then runs clippy + test.

### Frontend

From `frontend/` (or prefix with `pnpm --dir frontend`):

```bash
pnpm install --frozen-lockfile
pnpm dev                 # Vite dev server
pnpm build               # production build -> frontend/dist
pnpm typecheck           # tsc --noEmit
pnpm lint                # ESLint on src/
pnpm test                # Vitest once
pnpm test:watch          # Vitest watch
pnpm test:coverage       # Vitest with v8 coverage
```

### Full desktop app

From `apps/desktop/src-tauri/`:

```bash
cargo tauri dev          # launches Tauri with hot-reload frontend
cargo tauri build        # release bundle
```

The Tauri config at `apps/desktop/src-tauri/tauri.conf.json` hardcodes `"frontendDist": "../../../frontend/dist"`. Build the frontend first for a production desktop build.

### Default quality gates (run these before any PR)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
git diff --check
cargo deny check advisories bans licenses sources

# Default repository guard scripts (PowerShell)
powershell -ExecutionPolicy Bypass -File scripts/check-doc-drift.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-doc-archive.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-command-sql-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts/check-module-size.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts/check-rust-function-size.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-rust-test-layout.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts/check-rust-test-layout.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage0-boundary-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage2-platform-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage2-real-sample-isolation.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage3-command-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage4-service-boundary.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts/check-stage4-service-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-media-protocol-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-release-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-stage5-regression-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-frontend-lockfile-policy.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-deny-exceptions.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-evtx-dependency-decision.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-benchmark-regression.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-dependency-security.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-e01-import-performance.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-import-optimization-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1
```

## Code Style Guidelines

### Rust

- **Crate names**: kebab-case (`artifacts-windows`, `app-services`).
- **Module/file names**: snake_case (`case_service.rs`, `file_commands.rs`).
- **Workspace dependencies**: Centralized in root `Cargo.toml` `[workspace.dependencies]`. Member crates reference them with `{ workspace = true }`. Do not write direct version numbers in member `Cargo.toml` files.
- **Edition 2021** across all crates. Toolchain is stable with `rustfmt` + `clippy` (`rust-toolchain.toml`).
- **Error handling**:
  - New crates must define typed errors with `thiserror` (e.g. `LinuxArtifactError`, `PstError`). Do **not** return `Result<T, String>` in new code.
  - Cross-crate error DTO: `transport::errors::ApiErrorDto` with `code`, `message`, `category`, `details`, `recoverable`.
- **Formatting / lint**: `cargo fmt --all -- --check` must be zero diff; `cargo clippy --workspace --all-targets -- -D warnings` must produce no errors.
- **File size limits** (Stage 0 backend refactor):
  - Production Rust files target <= 500 lines; new non-baselined files above the target fail `check-module-size.ps1`, with 800 as the hard ceiling for new normal modules.
  - `mod.rs` / `lib.rs` files target <= 200 lines and should remain API / re-export surfaces.
  - Existing file-size violations are locked by `scripts/baselines/rust-module-size-baseline.csv` and may not grow.
  - Current post-cleanup module baseline is zero. Five reviewed normal-module exceptions remain and expire on 2026-09-30; new work must not copy these patterns or extend their expiry without architecture review.
  - New normal modules from 501 through 800 lines require a valid temporary `path/owner/reason/expires` entry in `scripts/baselines/rust-module-size-exceptions.csv`; migration baseline rows are not formal exceptions.
  - Production functions target <= 100 lines. All existing >100-line debt, including existing >150-line functions, is identity-locked by `scripts/baselines/rust-function-size-baseline.csv` and may only shrink. Any non-baselined function >100 fails; 150 is the new-code hard ceiling.
  - Module/function/test-layout baseline edits are transition-checked against their `RUST_*_BASELINE_REFERENCE` CI base SHA or `-ReferenceRevision`; rows may only decrease or be deleted. Initial baselines require the actual hash, revision/hash-pinned manifest, and PR-external protected `RUST_*_BOOTSTRAP_SHA256` repository variable to agree.
  - All three guards discover workspace members through `cargo metadata`; shared ordinal path/CSV/test-boundary policy lives in `scripts/lib/RustGuard.Common.ps1`. Zero-debt baselines are header-only, never empty files.
  - Every production Cargo target must remain inside its owning package `src/`; the exact package-root `build.rs` is the only exception. Target source is scanned regardless of extension, and every `src/**/*.rs` file is scanned regardless of test-like names or directories. Production `#[path] mod` and token-injecting `include!` are prohibited.
  - Only physical top-level Cargo `tests/`, `benches/`, and `examples/` trees are exempt from module/function limits. Files such as `src/tests.rs`, `src/*_tests.rs`, and `src/tests/**` remain migration debt and are not exempt.
  - Cargo metadata has a bounded asynchronous timeout; on Windows, exact-PID taskkill, a kill-on-close Job Object, and a bounded PID/parent/creation-time snapshot terminate the process tree without process-name kills. Windows physical file identities deduplicate case-insensitively; Linux remains case-sensitive. Unknown proc macros and cross-file wildcard test aliases require explicit guard review before use.
- **Dead code**: Do not use `#[allow(dead_code)]` in production code. Remove unused code. Parser format constants are exempt.
- **Unsafe**: Every `unsafe` block must have a `// SAFETY:` comment. Prefer RAII guards for FFI resources.
- **DTO contract**:
  - All serializable API types live in `crates/transport/src/dto/`. Never define them in other crates.
  - DTOs use `#[serde(rename_all = "camelCase")]`.
  - Optional fields use `#[serde(skip_serializing_if = "Option::is_none")]`.
  - Rust DTO types end in `Dto` (`CaseSummaryDto`); frontend interfaces drop the suffix (`CaseSummary`).

### Frontend

- **Path alias**: `@/` → `frontend/src/` (configured in `vite.config.ts` and `tsconfig.json`).
- **TypeScript**: strict mode enabled. `noEmit`, ES2020 target, bundler module resolution.
- **File naming**: PascalCase for components (`FileBrowser.tsx`), camelCase for hooks/utils (`hooks.ts`).
- **Component layout**:
  - Page components: `frontend/src/app/pages/`.
  - Reusable layout components: `frontend/src/components/layout/` (AppShell, Layout, TopBar, BottomDrawer, InspectorPane, PageSubbar).
  - shadcn/radix UI primitives: `frontend/src/app/components/ui/`.
  - Feature hooks: `frontend/src/features/<domain>/hooks.ts`.
  - API layer: `frontend/src/lib/api/<domain>.ts`.
  - Global state: Zustand stores in `frontend/src/stores/`.
- **Styling**: Tailwind CSS 4 is CSS-first via `@tailwindcss/vite`. Configuration lives in `frontend/src/styles/theme.css` and `tailwind.css`. Do **not** create a `tailwind.config.js`.
- **Pages must not call `invoke` directly**; route all calls through `apiClient.request(commandName, payload)`.
- **Component file size**: each component file ≤ 500 lines.

## Testing Instructions

### Rust

```bash
cargo test --workspace
```

- New Rust tests should live in physical `tests/` directories. Existing tests embedded under `src` are baseline-locked by `scripts/check-rust-test-layout.ps1`; after migration, `src` may only keep the exact non-public `#[cfg(test)]` + `#[path = "..."]` + `mod tests;` bridge whose source-relative target exists and canonically remains inside the owning crate/app `tests/unit/` directory. Top-level `tests/*.rs` files are integration entries and must not be used as bridges.
- Test categories:
  - DTO / serde round-trip tests (every DTO at least one).
  - Service unit tests (every public function at least one).
  - Repository / migration tests.
  - Parser tests: valid / invalid / edge (at least 3 per parser family).
  - Fixture / expected JSON regression tests.
  - Real E01 regression tests are marked `#[ignore]` and run by setting `FORENSICS_E01_FIXTURE` or `FORENSICS_LIUYANG_E01_FIXTURE` environment variables.
  - The six-member PVE cluster import regression is marked `#[ignore]`; set `FORENSICS_PVE_CLUSTER_ROOT` and run `scripts/check-pve-cluster-import.ps1`. It serially attempts all members, expects three host `disk01` filesystem imports and three metadata-only BlueStore `disk02` imports, and verifies source-database isolation, zero ordinary BlueStore file rows, sanitized OSD/BlueFS replay inventory, exact RocksDB CURRENT/IDENTITY/MANIFEST control-plane oracles, complete `35/40/33` live-SST single-pass inventories, exact active-WAL/WriteBatch metadata, 12 per-OSD column-family latest-state summaries with canonical aggregate hashes, plus key-file preview.

### Frontend

```bash
pnpm --dir frontend test
pnpm --dir frontend test:coverage
```

- Framework: Vitest with jsdom, `@testing-library/react`, `@testing-library/jest-dom`.
- Setup file: `frontend/src/test/setup.ts`.
- Test file pattern: `src/**/*.{test,spec}.{ts,tsx}`.
- Coverage thresholds (v8): lines/statements/functions 45%, branches 35%.
- Coverage directory: `frontend/coverage`.

### Repository guard scripts

PowerShell scripts in `scripts/` encode architectural and security boundaries:

| Script | Guards |
|--------|--------|
| `check-command-sql-boundary.ps1` | No raw SQL in Tauri command handlers |
| `check-module-size.ps1` | Backend Rust module-size debt may not add or grow beyond baseline |
| `check-rust-function-size.ps1` | Comment/string-aware Rust function debt guard; exact identity, reference-revision transition, target 100 and new-code hard ceiling 150 |
| `check-rust-test-layout.ps1` | Rust tests embedded under `src` may not add or grow beyond baseline |
| `check-stage0-boundary-guard.ps1` | Stage 0 frontend/runtime and backend dependency boundaries |
| `check-stage2-platform-boundary.ps1` | Domain platform ownership, symmetric analyzers, thin analysis command, and facade limits |
| `check-stage2-real-sample-isolation.ps1` | Opt-in Windows/Linux dual-E01 isolation in both serial import orders; `-RequireFixtures` makes missing private samples fatal |
| `check-stage3-command-boundary.ps1` | Thin Tauri command modules, transport ownership, and command facade limits |
| `check-stage4-service-boundary.ps1` | Application-service capability wiring, Tauri dependency boundary, lexical-safe forensic invariants, and self-test |
| `check-media-protocol-guard.ps1` | Media preview stays on `evidence-media:` protocol |
| `check-release-guard.ps1` | No debug strings in release paths |
| `check-stage5-regression-guard.ps1` | MCP transport validation, nested DTO contracts, staging merge conflicts |
| `check-frontend-lockfile-policy.ps1` | Frontend lockfile policy compliance |
| `check-deny-exceptions.ps1` | Cargo deny exception validity |
| `check-evtx-dependency-decision.ps1` | EVTX vendored dependency constraints |
| `check-doc-drift.ps1` | Documentation consistency; add `-RenderMermaid` to render diagrams |
| `check-doc-archive.ps1` | UTF-8 documentation, archive taxonomy, manifest counts, and routing targets |
| `check-benchmark-regression.ps1` | Benchmark threshold regression |
| `run-benchmark.ps1` | Benchmark harness |
| `run-liuyang-artifact-test.ps1` | Real E01 sample pipeline |
| `run-coverage.ps1` | Coverage harness |

## Security Considerations

### Evidence integrity

- Original evidence sources are read-only. All derived data is written only to the case workspace, SQLite database, index directories, or explicit export paths.
- File extraction and report export default to `overwrite=false`. Target existence returns a conflict rather than silently overwriting.
- Path validation must happen before any write. Prefer temp file + atomic rename.

### Path and media safety

- Do not construct host filesystem paths from case roots + file entry paths for evidence preview. Use the evidence reader path keyed by `FileEntryId`.
- Media preview uses the custom `evidence-media://handle/<encoded>` protocol registered by Tauri. The CSP in `tauri.conf.json` allows `media-src 'self' data: evidence-media:`.
- Media handles are short-lived, bounded tokens; never expose raw host paths.
- Any media range request must validate handle, offset, and length.

### MCP security

- MCP is a controlled extension channel, not arbitrary execution.
- SSE transports only allow `http/https`, no embedded credentials.
- Stdio commands must be executable names only, not paths.
- Default least privilege:
  - `resourceAccess = readOnly`
  - `toolAccess = disabled`
  - `promptAccess = readOnly`
  - `networkPolicy = localhostOnly`
- MCP critical actions must be audited. MCP outputs entering the UI or reports must preserve source boundaries.

### Dependency governance

- `deny.toml` defines the dependency policy. `cargo deny check advisories bans licenses sources` must pass.
- Every advisory/license/ban exception must include `owner`, `reason`, and `expires` date. Expired exceptions fail CI (`check-deny-exceptions.ps1`).
- Allowed licenses are listed in `deny.toml`.
- Frontend lockfile policy is enforced by `check-frontend-lockfile-policy.ps1`.

### Error desensitization

- Errors follow the taxonomy in `docs/error-taxonomy.md` and `docs/error-classification-manual.md`.
- Sensitive host paths, credentials, and internal stack traces must not leak to the UI or reports.
- Use `ApiErrorDto` with an appropriate `ErrorCategory` (`validation`, `unsupported`, `io`, `parser`, `security`, `external`, `timeout`, `internal`).

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│  React UI (frontend/)                                   │
│  Vite + Tailwind 4 + React 18 + React Router 7          │
│  State: Zustand stores + TanStack Query                 │
├─────────────────────────────────────────────────────────┤
│  Tauri Command Layer (apps/desktop/src-tauri/)          │
│  Thin wrappers: validate -> service -> DTO              │
├─────────────────────────────────────────────────────────┤
│  Application Services (crates/app-services/)            │
│  Use-case orchestration per domain                      │
├─────────────────────────────────────────────────────────┤
│  Core Crates                                            │
│  domain / evidence-core / fs-* / image-* / search /     │
│  timeline / artifacts-* / catalog / reports /           │
│  persistence-sqlite / infrastructure / exchange         │
└─────────────────────────────────────────────────────────┘
```

### Dependency direction

```text
domain / transport  <-  app-services  <-  Tauri commands
       ^                     ^
       |                     |
 persistence         evidence / search / timeline / artifacts / reports
```

- `crates/transport` is the single source of truth for the frontend↔backend IPC contract. There is no codegen.
- Parser / repo / core crates must not depend on Tauri or the frontend.
- `AppState` (`apps/desktop/src-tauri/src/state/app_state.rs`) holds the active case, task manager, SQLite pool, MCP clients/config, and settings paths.

### Data Flow (request path)

1. Frontend hook calls `apiClient.request(commandName, payload)`.
2. `apiClient` invokes the Rust command via `@tauri-apps/api/core` → `invoke`.
3. Tauri command (`#[tauri::command]`) validates and delegates to `app-services`.
4. Service returns a DTO from `crates/transport/src/dto/`.

### Event Flow (push path)

Backend → Frontend via Tauri `emit`. Topics are string constants in `crates/transport/src/events/mod.rs`, mirrored as the TypeScript `EventTopic` union in `frontend/src/types/models.ts`. Frontend subscribes through `EventBus` (`frontend/src/lib/events/bus.ts`).

## Workspace Structure (verified counts)

| Count | Location | Notes |
|-------|----------|-------|
| 36 crates | `crates/*` workspace members | The Tauri shell is a separate workspace package |
| 98 Tauri commands | `apps/desktop/src-tauri/src/commands/**/*.rs` | Registered in `src/lib.rs` |
| 24 SQLite repositories | `crates/persistence-sqlite/src/repositories/*_repo.rs` | Includes datasource_cluster_repo, ceph_osd_repo, ceph_bluefs_repo, ceph_bluefs_replay_repo, ceph_rocksdb_repo, ceph_rocksdb_sst_repo, ceph_rocksdb_wal_repo, ceph_rocksdb_latest_state_repo, ceph_bluestore_semantic_repo |
| 49 migration scripts | `crates/persistence-sqlite/src/migrations/scripts/*.sql` | `0001`–`0036`, `source_001`–`source_012`, plus `staging_001.sql` |
| 10 frontend pages | `frontend/src/app/pages/*.tsx` (excluding `*.test.tsx`) | Includes V2 Workbench, V3 Dashboard, CaseHome, FileBrowser, etc. |
| 86 frontend test files | `frontend/src/**/*.test.{ts,tsx}` | |
| ~2,061 Rust tests | `cargo test --workspace` (calibrated 2026-06) | |
| 19 event topics | `crates/transport/src/events/mod.rs` | File extract progress added in 2026-06 |
| 33 DTO domain files | `crates/transport/src/dto/*.rs` | Includes analysis_browser.rs added in 2026-06 |

## V5 Quality Audit (2026-06)

The project underwent a comprehensive quality audit across 5 stages:
- **Stage 1 (Critical)**: MFT code deduplication, `unimplemented!()` elimination, TOCTOU race condition fix
- **Stage 2 (Architecture)**: gql/ingest trait decoupling, ~83 raw SQL statements migrated to repository layer, timestamp deduplication
- **Stage 3 (Runtime)**: Typed error migration (209→11 `Result<String>`), god function decomposition (418→86 lines), unwrap elimination in FS parsers
- **Stage 4 (Quality)**: Tauri command business logic extraction, documentation sync, React.memo on 9 components
- **Stage 5 (Forensics)**: EVTX expanded to 7 channels, NTFS EFS detection, browser profile auto-detection, RecycleBin v1 support

Key architectural improvements:
- **E01 preview pipeline**: Uses stored partition metadata instead of re-probing, MFT inode-based file resolution bypasses 8.3 short-name INDX issues
- **Path reconstruction**: File paths now include partition index prefix `[P{n}]` for unambiguous multi-partition resolution
- **Chunk-table caching**: `E01Reader::re_open()` shares `Arc<chunk_table>` across previews with independent file handles

### Crate roles

| Crate | Role |
|-------|------|
| `domain` | Core entities: CaseId/CaseMeta/CaseSession, DataSource, FileEntry, Artifact, TimelineEvent, Job, Report, Tag |
| `transport` | Shared DTOs, commands, events, errors, paging — the IPC contract |
| `app-services` | Application-layer orchestration per domain entity |
| `persistence-sqlite` | SQLite repositories and migrations |
| `evidence-core` | Disk image probing, volume detection, filesystem abstraction, reader; APFS/HFS+ type recognition is metadata-only and unsupported |
| `fs-ntfs` / `fs-fat` / `fs-exfat` / `fs-ext4` / `fs-xfs` / `fs-btrfs` | Filesystem-specific parsers |
| `fs-lvm` | Linux LVM physical-volume/logical-volume mapping and offset translation |
| `ceph-wire` | Read-only Ceph BlueFS superblock and transaction-log wire decoding |
| `rocksdb-wire` | Read-only RocksDB physical log and WriteBatch decoding, VersionEdit/MANIFEST replay, single-pass block-based live-SST streaming, and bounded latest-state reduction primitives |
| `image-raw` / `image-e01` | Raw and E01 image format readers |
| `search` | Full-text indexing (tantivy), query parsing, highlighting |
| `timeline` | Timeline event generation and aggregation |
| `artifacts-core` | Artifact extraction framework |
| `artifacts-windows` | Windows artifact parsers: Browser, EVTX, Prefetch, LNK, JumpList, Registry (SYSTEM/SOFTWARE/NTUSER/SAM/USRCLASS/Amcache/SECURITY/txlog), RecycleBin, SRU, Thumbcache |
| `artifacts-linux` | Linux artifact parsers: systemd journal, wtmp, bash history, apt/dpkg, cron, sudo |
| `artifacts-ios` / `artifacts-android` | Mobile artifact parser placeholders |
| `containers-pst` | PST/OST/mbox email container parsing |
| `catalog` | File catalog indexing with ExtensionProjection, PathPrefixProjection, CatalogIndex |
| `reports` | Report generation: HTML, CSV, JSON, evidence bundle |
| `exchange` | STIX 2.1 exchange engine, entity resolution, cross-case matching, Ed25519 signing, chain-of-custody, UCO mapping |
| `ingest` | Ingestion pipeline orchestration — `IngestPipeline` trait, `IngestConfig`, `IngestSink`, `IngestStats` |
| `mcp-client` | MCP client — SSE and Stdio transports |
| `runtime-cache` | Handle-based runtime cache (not source of truth) |
| `infrastructure` | Cross-cutting: logging, hashing, filesystem utils, text, clock, config |
| `testing` | Test builders and fixtures |
| `evtx-patched` | Vendored/patched EVTX parser (consumed as `evtx`) |
| `gql` / `updater` / `cloud-audit` / `crash_handler` | Graph query surface, updater, cloud audit, crash handling |

### Registry module structure (`crates/artifacts-windows/src/registry/`)

```text
lookup/
  mod.rs           — module root and re-exports
  types.rs         — hive cell types (HBIN, NK, VK, SK, LF, LH, RI, LI), value types, F struct
  reader.rs        — registry hive file reader, cell navigation, dirty page merging
  txlog_util.rs    — .LOG1/.LOG2 transaction log parser, dirty page bitmap, page recovery
  utf16.rs         — UTF-16LE key/value name decoding from hive cells
  system.rs        — SYSTEM hive extractor
  software.rs      — SOFTWARE hive extractor
  ntuser.rs        — NTUSER.DAT extractor
  sam.rs           — SAM hive extractor (RID is in VK data_type field, see Gotchas)
  security.rs      — SECURITY hive extractor (local security policy, LSA Secrets metadata, cached credentials — encrypted blobs only)
  shellbags.rs     — USRCLASS ShellBags extractor
  muicache.rs      — USRCLASS MuiCache extractor
  amcache.rs       — Amcache.hve extractor (InventoryApplication, InventoryApplicationFile)
  hash_decrypt.rs  — SAM LM/NT hash decryption (BootKey → hashedBootKey → rid2key DES)
```

## Conventions

### Adding a new feature (typical flow)

1. Define/extend DTOs in `crates/transport/src/dto/<domain>.rs`, then re-export from `mod.rs`.
2. Add command request types in `crates/transport/src/commands/mod.rs` if needed.
3. Implement service logic in `crates/app-services/src/<domain>_service.rs` (or `service/<domain>/mod.rs` if large).
4. Wire Tauri command in `apps/desktop/src-tauri/src/commands/<domain>_commands.rs`.
5. Register command in `apps/desktop/src-tauri/src/lib.rs` `invoke_handler`.
6. Mirror DTO as TypeScript interface in `frontend/src/types/models.ts`.
7. Add API function in `frontend/src/lib/api/<domain>.ts`.
8. Create/update hook in `frontend/src/features/<domain>/hooks.ts`.
9. Build page/component consuming the hook.
10. Update `README.md`, `AGENTS.md`, `CLAUDE.md`, and `docs/documentation-index.md` if crate/command/migration counts change.

### Parallelism (Rayon)

- Import `rayon::prelude::*`.
- Use `par_iter()` for CPU-bound batch operations (artifact extraction, correlation matching, timeline MACB projection, hashing).
- Keep I/O-bound work serial to avoid contention on the E01 reader.
- Shared mutable state needs `Mutex<T>` with small lock granularity.
- Closures passed to `par_iter()` must be `Sync`.
- Sort after parallel collection for deterministic output.
- SQLite `Connection` is not `Sync`; each thread must open its own connection.

## Key Design Documents

- `PRD.md` — Product requirements
- `spec.md` — Technical specification
- `design.md` — Detailed architecture, crate responsibilities, data structures, MVP phases
- `ci.md` — CI pipeline design (GitHub Actions structure, check steps, caching rules)
- `test-plan.md` — Testing strategy
- `autopsy-borrowings.md` — Concepts borrowed from Autopsy

工程化/专题文档 (engineering and topic docs):

- `docs/development-engineering-guide.md` — 开发流程与工程约定
- `docs/engineering-audit-plan.md` — 可执行的全量审计清单
- `docs/design-constraints.md` — 架构、证据、安全、前端与发布约束
- `docs/documentation-index.md` — 权威文档入口、事实快照、历史归档规则
- `docs/progress-ledger.md` — 当前里程碑、真实样本基线与下一开发边界
- `docs/archive/README.md` — 按类型/月份组织的历史文档入口与归档规则
- `docs/model-architecture-algorithm-diagrams.md` — Mermaid 图谱合集
- `docs/v2-longterm-plan.md` — V2 阶段边界、测试矩阵、评分机制
- `docs/validation-trust-framework.md` — public fixture、expected JSON、真实样本回归
- `docs/fixture-handbook.md` — fixture 分层与元数据要求
- `docs/expected-json-contract.md` — expected JSON 字段承诺规则
- `docs/parser-support-matrix.md` — 支持边界与验证样本
- `docs/known-unsupported-formats.md` — 明确不支持/部分支持的格式
- `docs/error-taxonomy.md` / `docs/error-classification-manual.md` — 错误分类与脱敏
- `docs/benchmark-baseline.md` — benchmark 数据集分级与阈值
- `docs/correlation-analysis-design.md` — 关联模型与调查工作流
- `docs/release-scorecard.md` — 发布评分卡与硬门禁
- `docs/mcp-security-model.md` / `docs/mcp-user-guide.md` — MCP 权限与使用
- `docs/export-and-media-safety.md` — 导出路径、overwrite、media handle
- `docs/v3-plan.md` / `docs/v3-walkthrough.md` — V3 主计划与演练
- `docs/v4-plan.md` — V4 主计划

Governance fact sources (embedded at compile time, drive `/v2` governance snapshot):

- `testdata/governance/v2-verification-catalog.json`
- `testdata/governance/v2-benchmark-baseline.json`
- `testdata/governance/v2-known-limitations.json`
- `testdata/governance/v2-release-policy.json`
- `testdata/governance/v2-runtime-results.json`
- `testdata/governance/v2-security-taxonomy.json`

## Gotchas

1. **Transport crate is the manual contract**: DTOs are in `crates/transport/src/dto/`. There is no codegen. Every change must be mirrored manually in `frontend/src/types/models.ts`. Event topics in `crates/transport/src/events/mod.rs` must stay in sync with the TypeScript `EventTopic` union.

2. **No mock mode**: The frontend always invokes real Tauri commands. `pnpm dev` requires the Tauri environment to function; use `cargo tauri dev` for a full desktop development loop.

3. **Frontend dist path**: Tauri expects the built frontend at `frontend/dist` relative from `src-tauri/`, hardcoded as `"../../../frontend/dist"` in `tauri.conf.json`.

4. **Tauri 2**: Commands use `#[tauri::command]` with the v2 handler registration pattern. Events use the `Emitter` trait.

5. **SAM RID location**: When extracting SAM user information, the RID is stored in the VK cell's `data_type` field (4-byte DWORD), not in the value data payload.

6. **Partition root model**: Each partition has exactly one visible root node in the main database. The first-level tree must not expose raw `\`, `/`, or `.` as roots. Root folding happens in the import/staging merge path, not in the frontend.

7. **File browsing state**: `deleted`, `hidden`, `system` are real backend fields shared between tree and list. Sorting is "directories first + status after + natural name"; the backend is the source of truth.

8. **Expected JSON is the contract**: When parser output changes, update the corresponding expected JSON in `testdata/fixtures/` first, then update the parser. CI regression gates compare parser output against expected JSON.

9. **Graph population is non-fatal**: Graph node/edge writes use non-fatal error handling. A graph write failure does not abort import. Check `graph_nodes`/`graph_edges` counts after import.

10. **Entity merge before cross-case matching**: `EntityMergeEngine::deduplicate_entity_nodes` populates `resolved_entities`. `CrossCaseEntityMatcher::match_entities_across_cases` reads from this table and requires at least 2 databases.

11. **Ed25519 signing is deterministic and stateless**: `SigningEngine` is a pure namespace. The signing payload for case exports is `SHA-256(case_id || timestamp || case_content_hash)`.

12. **Custody chain hash linking**: `ChainOfCustody` entries form a hash chain via `prev_hash`. Append with `append_entry_after` using the verified previous entry hash. The Merkle tree provides an independent batch verification path.

13. **MBR partitions**: Use `parse_mbr_full()` (not `parse_partition_table()`) for MBR-aware code; effective indices are computed from candidate offset order because MBR lacks GPT `partition_index`.

14. **Module split pattern**: Organize a capability family as a directory of single-purpose production files. `mod.rs` is limited to declarations, public API, and re-exports. Test bodies belong in the owning crate/app `tests/` directory, never in `src/tests.rs` or inline `#[cfg(test)]` modules.

15. **Correlation depends on `sourceObjectId`**: The primary Artifact↔Timeline correlation bridge uses shared `sourceObjectId`. Every new artifact extractor must set this field or cross-artifact leads will silently miss connections.

16. **Rule families derive from artifact type**: `families[]` on `CorrelationLeadDto` / `CorrelationClusterDto` is derived from artifact type, not hardcoded. Adding a new artifact type requires updating family derivation logic and the governance catalog.

17. **Benchmark baselines are host-specific**: `v2-benchmark-baseline.json` thresholds are calibrated for the reference host. `v2-runtime-results.json` records the last run's host configuration — check it before interpreting regressions.

18. **Dead code is not allowed**: Do not add `#[allow(dead_code)]` in production code. Parser format constants are the main exemption.

19. **No direct SQL in commands**: Keep SQL in repositories / services. `check-command-sql-boundary.ps1` enforces this.

20. **Chinese docs are authoritative for several topics**: `docs/development-engineering-guide.md`, `docs/design-constraints.md`, `docs/engineering-audit-plan.md`, and `docs/documentation-index.md` contain authoritative engineering constraints. If a Chinese doc conflicts with an older English doc, the Chinese engineering doc and `AGENTS.md` take precedence.

21. **MCP DTOs use snake_case intentionally**: The 12 DTOs in `crates/transport/src/dto/mcp.rs` intentionally lack `#[serde(rename_all = "camelCase")]`. This is guarded by `check-stage5-regression-guard.ps1`. Frontend uses `*ProtocolDto` interfaces to adapt. Do NOT add camelCase to MCP DTOs without updating the regression guard and frontend normalizer.

22. **Batch execution control is a V3 stub**: `create_batch_plan`, `get_batch_job`, and `list_batch_jobs` are implemented. `start_batch`, `pause_batch`, `resume_batch`, and `cancel_batch` return `BatchServiceError::Unsupported` until V3 scheduling lands.

23. **Platform admission is backend-owned**: Windows and Linux are the only production analysis platforms. Do not reintroduce a macOS DTO variant, extractor category, governance coverage field, or UI tab. Legacy `platform='macos'` cases must fail with typed `Unsupported`; APFS/HFS+ detection must never instantiate a content reader.
