# AGENTS.md

## Project Overview

**Forensics Workbench** — a Tauri 2 desktop application for disk image forensic analysis on Windows. Rust backend handles evidence processing (disk images, file systems, Windows artifacts, search indexing, timeline generation). React/TypeScript frontend provides the investigator UI.

Single-user, desktop-first, Windows-primary. No HTTP server — all frontend↔backend communication goes through Tauri commands and events.

**V2 Status: ~90% complete, Grade B (81/100).** All 7 real E01 regression tests pass. Four stages (V2-1 through V2-4) cover verifiable trust (95%), cross-artifact correlation (85%), performance/scale (70%), and security governance/release (75%). The `/v2` governance dashboard surfaces live scores from correlation signals, support matrix, error taxonomy, benchmark thresholds, and release gates. Governance fact sources live in `testdata/governance/`.

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
| `persistence-sqlite` | SQLite repositories and migrations (12 repos, 31 migration scripts) |
| `evidence-core` | Disk image probing, volume detection, filesystem abstraction, reader |
| `fs-ntfs` / `fs-fat` / `fs-exfat` | Filesystem-specific parsers |
| `fs-ext4` / `fs-xfs` / `fs-btrfs` / `fs-apfs` / `fs-hfsplus` | Additional filesystem parsers for Linux/macOS filesystems |
| `image-raw` / `image-e01` | Raw and E01 image format readers |
| `search` | Full-text indexing (tantivy), query parsing, highlighting |
| `timeline` | Timeline event generation and aggregation |
| `artifacts-windows` | Windows artifact parsers: Browser (Chrome/Edge/Firefox), EVTX, Prefetch, LNK, JumpList, Registry, RecycleBin, SRU, Thumbcache |
| `artifacts-linux` | Linux artifact parsers: systemd journal, wtmp, bash history, apt/dpkg, cron, sudo |
| `artifacts-macos` | macOS artifact parsers: plist, unified log, Spotlight, Quarantine, Launch Services, FSEvents |
| `containers-pst` | PST/OST/mbox email container parsing (Unicode 32/64, RFC 4155 mbox variants) |
| `catalog` | File catalog indexing with ExtensionProjection, PathPrefixProjection, CatalogIndex |
| `reports` | Report generation: HTML, CSV, JSON, evidence bundle |
| `exchange` | STIX 2.1 exchange engine with Ed25519 signing, chain-of-custody, and UCO case mapping |
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

3. **Transport crate is the contract**: DTOs are in per-domain files under `crates/transport/src/dto/` (case.rs, files.rs, search.rs, timeline.rs, artifacts.rs, jobs.rs, viewer.rs, reports.rs, exchange.rs, entity_resolution.rs). Any change must happen here first. Both the Tauri command layer and the frontend `types/models.ts` must stay in sync manually — there is no codegen yet.

4. **domain crate is implemented**: Core types (CaseMeta, FileEntry, Artifact, TimelineEvent, Job, Report, Tag, DataSource) are defined with serde support. Most crates are fully implemented: `persistence-sqlite` (12 repos, 31 migration scripts), `evidence-core` (image probing, volume detection), `fs-ntfs`/`fs-fat`/`fs-exfat`/`fs-ext4`/`fs-xfs`/`fs-btrfs`/`fs-apfs`/`fs-hfsplus` (filesystem parsers), `artifacts-windows` (9 extractors), `search` (tantivy indexing), `catalog` (ExtensionProjection, PathPrefixProjection), `exchange` (STIX 2.1 signing + custody + UCO), `entity_resolution` (merge engine + cross-case matching), `ingest` (pipeline trait), `mcp-client` (SSE + Stdio transports).

5. **Frontend test framework**: Vitest is configured with jsdom environment. Run `pnpm test` from `frontend/`. Current source tree has 43 frontend test files covering pages (Settings, DataAnalysis, FileBrowser, Search, Timeline, Artifacts, Reports, V2Workbench), viewers (Hex, Text, Image), stores (ui-store, selection-store, mcp-store), API layer, events, routes, and hooks. Coverage thresholds: 45% branches, 35% functions/lines/statements.

6. **Tailwind 4 with `source(none)`**: The Tailwind config uses `@import 'tailwindcss' source(none)` with explicit `@source` directive. Don't add a `tailwind.config.js` — configuration is CSS-first.

7. **Event topics are string constants**: Defined in `crates/transport/src/events/mod.rs` and mirrored as a TypeScript union type `EventTopic` in `src/types/models.ts`. Keep them in sync.

8. **Tauri 2**: This project uses Tauri v2 (not v1). Commands use `#[tauri::command]` with the v2 handler registration pattern. The `Emitter` trait is used for events.

## V2 Specific Gotchas

1. **Governance fact sources are canonical**: The `/v2` dashboard and `V2GovernanceSnapshotDto` derive from JSON files in `testdata/governance/`. Updating `v2-release-policy.json` score policies or `v2-known-limitations.json` immediately changes the governance dashboard and release scorecard. Do not edit these by hand without understanding the pipeline implications.

2. **Correlation rules depend on `sourceObjectId`**: The primary correlation bridge between Artifact↔Timeline uses shared `sourceObjectId`. If a parser does not set `sourceObjectId` correctly, cross-artifact leads will silently miss connections. Every new artifact extractor must set this field.

3. **Expected JSON is the contract, not the implementation**: When a parser output changes, update the corresponding expected JSON in `testdata/fixtures/` FIRST, then update the parser. The CI regression gate compares parser output against expected JSON — mismatches block merges.

4. **Release scorecard is derived, not static**: The `releaseScorecard` in the governance snapshot is computed from `releaseGates` + `runtimeSignals` (correlation signals, family coverage, benchmark thresholds). Adding a new rule family or benchmark dataset automatically affects the scorecard through runtime signals.

5. **Benchmark baselines are host-specific**: `v2-benchmark-baseline.json` stores thresholds calibrated for the reference host. Running benchmarks on a different machine produces invalid comparisons. The `v2-runtime-results.json` records the last run's host configuration — always check this before interpreting benchmark regressions.

6. **V2-2 rule families map to artifact types**: The `families[]` field on `CorrelationLeadDto` and `CorrelationClusterDto` is derived from artifact type (not hardcoded). Adding a new artifact type requires adding corresponding family derivation logic in `correlation_service` and updating the governance catalog.

## V3 Specific Gotchas

1. **MBR partitions need unique indices**: MBR disks lack GPT partition_index. The import pipeline and file viewer now compute effective indices from candidate offset order. When adding new MBR-aware code, use `parse_mbr_full()` not `parse_partition_table()`.

2. **Rayon requires Sync closures**: `artifact_service::run_extractors_parallel` now requires `file_reader: &(dyn Fn(...) + Sync)`. All existing callers (closures) are automatically Sync. New callers passing non-Sync state will fail to compile.

3. **Correlation sub-modules**: `correlation_service.rs` is now split into `correlation/{mod.rs, rules.rs, graph.rs, tests.rs}`. Import as `app_services::correlation::*`.

4. **Report sub-modules**: `report_service.rs` is now split into `report/{mod.rs, html.rs, csv.rs, json.rs, tests.rs}`. Import as `app_services::report::*`.

5. **Graph population is non-fatal**: Graph node/edge writes in file/artifact/timeline/correlation services use non-fatal error handling — a graph write failure does not abort the import. Check `graph_nodes`/`graph_edges` counts after import to verify completeness.

6. **New crates need thiserror**: `artifacts-linux` and `containers-pst` now use typed errors (`LinuxArtifactError`, `PstError`). New parsers should follow this pattern — no raw `Result<T, String>` returns.

## V4 Specific Gotchas

1. **Entity merge must precede cross-case matching**: `EntityMergeEngine::deduplicate_entity_nodes` populates the `resolved_entities` table. `CrossCaseEntityMatcher::match_entities_across_cases` reads from this table. Running cross-case matching without per-case deduplication produces no matches because unresolvable entities never appear in `resolved_entities`.

2. **Entity merge re-points graph edges before deletion**: `deduplicate_entity_nodes` updates graph_edges (both `source_id` and `target_id`) from merged nodes to the kept node before deleting merged nodes. If you insert graph_edges manually outside the merge path, those edges will be lost on merge — they won't be automatically re-pointed.

3. **Cross-case matching requires at least 2 databases**: `CrossCaseEntityMatcher::match_entities_across_cases` returns an error with fewer than 2 `PathBuf` arguments. Single-case entity resolution uses the intra-case `EntityMergeEngine`, not the cross-case matcher.

4. **STIX 2.1 export maps transport DTOs directly**: Functions in `exchange/src/stix.rs` (`indicator_from_lead`, `observed_data_from_artifact`, `observed_data_from_registry`, `observed_data_from_email`) consume DTO types from `crates/transport`. Adding fields to `CorrelationLeadDto`, `ArtifactRowDto`, `RegistryValueDto`, or `EmailMessageDto` requires updating the STIX export mappings in the exchange crate.

5. **Ed25519 signing is deterministic and stateless**: `SigningEngine` uses Ed25519 — the same key and data always produce the same signature. The engine is a pure namespace (all methods take no `&self`), not a service with configuration or session state. The signing payload for case exports is `SHA-256(case_id || timestamp || case_content_hash)`.

6. **Custody chain entries are sequentially linked by prev_hash**: `ChainOfCustody` entries form a hash chain via `prev_hash`. Appending an entry out of sequence or with an incorrect `prev_hash` causes `verify_chain` to fail. Always use `append_entry_after` with the verified previous entry hash. The Merkle tree (`MerkleTree`, `MerkleProof`) provides an independent batch verification path.

7. **New filesystem crates share the evidence-core trait contract**: `fs-ext4`, `fs-xfs`, `fs-btrfs`, `fs-apfs`, and `fs-hfsplus` implement the `FileSystemReader` trait from `evidence-core`. When modifying parser behavior, ensure consistent metadata output (MIME types, timestamps, file sizes) across all filesystem crates. Test suites for these crates run on synthetic filesystem images (12 samples for HFS+/APFS/Btrfs, fewer for ext4/XFS).

## Key Design Documents

- `PRD.md` — Product requirements
- `spec.md` — Technical specification
- `design.md` — Detailed architecture, crate responsibilities, data structures, MVP phases
- `ci.md` — CI pipeline design (GitHub Actions structure, check steps, caching rules)
- `test-plan.md` — Testing strategy
- `autopsy-borrowings.md` — Concepts borrowed from Autopsy (reference forensic tool)

补充工程化文档：

- `docs/engineering-audit-plan.md` - 可执行的项目工程化全量审计清单
- `docs/development-engineering-guide.md` - 开发流程与工程约定
- `docs/design-constraints.md` - 架构、证据、安全、前端与发布约束
- `docs/model-architecture-algorithm-diagrams.md` - 模型、架构与算法 Mermaid 图谱
- `docs/documentation-index.md` - 当前权威文档入口、旧文档去重索引与事实校准记录
- `docs/v2-longterm-plan.md` - V2 长期执行计划、阶段边界、评分与验收标准
- `docs/fixture-handbook.md` - fixture 分层、目录规范与样本元数据要求
- `docs/expected-json-contract.md` - expected JSON 断言结构与字段承诺规则
- `docs/error-classification-manual.md` - 错误分层、脱敏与审计实施口径
- `docs/benchmark-baseline.md` - benchmark 数据集分级、指标口径与阈值
- `docs/correlation-analysis-design.md` - 多工件关联分析设计与调查工作流
- `docs/release-scorecard.md` - 发布评分卡、硬门禁与发布材料要求

治理事实源（编译时嵌入，驱动 /v2 governance snapshot）：
- `testdata/governance/v2-verification-catalog.json`
- `testdata/governance/v2-benchmark-baseline.json`
- `testdata/governance/v2-known-limitations.json`
- `testdata/governance/v2-release-policy.json`
- `testdata/governance/v2-runtime-results.json`

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
