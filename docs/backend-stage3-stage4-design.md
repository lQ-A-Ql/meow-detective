# Backend Stage 3 and Stage 4 Delivery Design

## Purpose

This document is the executable design for the backend refactor after platform
isolation. The baseline is commit `7ac7e695`.

- Stage 3 splits transport requests and restores Tauri commands to thin IPC
  adapters.
- Stage 4 splits application-service god modules by stable use case while
  preserving public APIs and forensic behavior.

Neither stage changes evidence formats, file-preview semantics, source database
isolation, platform routing, or frontend DTO shapes.

## Engineering Baseline

The following boundaries are mandatory:

- Production Rust files target 500 lines; new ordinary modules may not exceed
  800 lines.
- `mod.rs`, `lib.rs`, and compatibility facades target 200 lines.
- Production functions target 100 lines and may not introduce new debt.
- Test bodies live in physical `tests/` directories.
- Commands may validate requests, acquire application state, invoke a service,
  translate a typed error, and emit an event.
- Commands may not execute SQL, parse evidence, reconstruct paths, aggregate
  domain results, or choose platform capabilities.
- Existing Tauri command names and transport serde shapes remain stable.
- Files are UTF-8 without BOM and use LF line endings.

## Stage 3: Transport and Command Layer

### Stage Design

Stage 3 cuts two independent forms of coupling:

1. `transport::commands` is split by request domain while preserving root
   re-exports.
2. desktop commands are split by command family while preserving the module
   paths imported by `src/lib.rs`.

The compatibility facade is intentional. Call sites continue importing
`transport::commands::CreateCaseRequest` and
`commands::file_commands::read_file_range`; only implementation ownership
changes.

### Phase 3.1: Transport Request Domains

Tasks:

- Move case lifecycle requests to `commands/case.rs`.
- Move file browsing, viewer, extraction, and file search requests to
  `commands/files.rs`.
- Move import requests and source-kind DTOs to `commands/import.rs`.
- Move analysis requests to `commands/analysis.rs`.
- Move timeline, artifacts, report/export, settings, and remaining request
  families to dedicated files.
- Keep platform conversion in `commands/platform.rs`.
- Keep shared paging and path-validation helpers private to the narrowest
  domain possible.
- Reduce `commands/mod.rs` to module declarations and public re-exports.

Expected result:

- Request validation remains compatible at the API boundary.
- No frontend TypeScript change is required.
- `commands/mod.rs` stays below 200 lines.

### Phase 3.2: File Commands

Tasks:

- Split tree/row/jump commands from viewer and extraction commands.
- Split text/image/media/hex range adapters by protocol responsibility.
- Keep preview counters and state acquisition in a small support module.
- Delegate evidence reads to `file_service`; do not reconstruct evidence paths
  in commands.
- Preserve all registered Tauri function names.

Expected result:

- `file_commands.rs` becomes a re-export facade below 200 lines.
- Viewer safety, media handles, range validation, and source routing remain
  unchanged.

### Phase 3.3: Case Commands

Tasks:

- Split create/open/demo lifecycle, close/drain, recent cases, metrics, and data
  source management.
- Preserve create/open active-state ordering and create rollback behavior.
- Preserve delete cancel/drain/close/cache cleanup ordering.
- Keep recent-case persistence isolated from active-case lifecycle code.

Expected result:

- `case_commands.rs` becomes a re-export facade below 200 lines.
- No lifecycle or deletion regression is introduced.

### Phase 3.4: Remaining Heavy Commands

Tasks:

- Split MCP DTO mapping, configuration, connection lifecycle, resources, tools,
  and prompts.
- Split analysis query/extraction/governance command groups.
- Split batch lifecycle and benchmark harness adapters.
- Reuse shared command support rather than duplicating active-case and blocking
  adapters.

Expected result:

- Command files express one command family.
- Command modules contain no repository SQL or parser orchestration.

### Phase 3.5: Stage 3 Review and Gates

Review dimensions:

- Architecture 25
- Modularity 20
- Contract 15
- Robustness 15
- Tests 15
- Performance 10

The stage cannot be committed below 90 total, below 80 percent in any
dimension, or with an unresolved High/Critical finding.

Required gates:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
check-command-sql-boundary.ps1
check-stage3-command-boundary.ps1
check-module-size.ps1
check-rust-function-size.ps1
check-rust-test-layout.ps1
git diff --check
```

## Stage 4: Application Services

Implementation status: completed. The original Stage 4 delivery after Stage 3
commit `c3ae351` split timeline, staging, parallel enumeration, import
pipeline, file service, correlation, artifact, graph, entity, and rule-pack
services. The 2026-07-12 closure pass removed the remaining app-services
module/function baseline debt across analysis extraction, LVM probing,
governance scoring, notebook, report generation, datasource probing,
enumeration, import-analysis workers, and step replay.

Closure metrics:

- app-services module-size baseline rows: 7 to 0;
- app-services function-size baseline rows: 20 to 0;
- repository module-size baseline rows: 24 to 17;
- repository function-size baseline rows: 37 to 17;
- historic functions above 150 lines: 9 to 1.

### Stage Design

Stage 4 is a behavior-preserving use-case split. It does not move parser
algorithms between crates and does not expand public service APIs merely for
tests. Existing public entry points remain as small facades until callers can
move safely.

Work is ordered by shared dependency risk:

1. timeline and staging foundations;
2. parallel enumeration and import pipeline;
3. file service and viewer orchestration;
4. correlation, graph, artifact, entity, and rule-pack services.

### Phase 4.1: Timeline and Staging

Timeline target structure:

```text
timeline_service/
  mod.rs
  query.rs
  projection.rs
  pagination.rs
  export.rs
  error.rs
```

Staging target structure:

```text
staging/
  mod.rs
  schema.rs
  writer.rs
  merge.rs
  partition_root.rs
  cleanup.rs
  error.rs
```

Tasks:

- Separate timeline queries from event projection and DTO conversion.
- Separate staging schema creation, buffered writes, merge, root folding, and
  cleanup.
- Keep SQL in repositories or explicit persistence helpers.
- Preserve transaction boundaries and deterministic event ordering.

### Phase 4.2: Parallel Enumeration and Import Pipeline

Parallel enumeration target structure:

```text
parallel_enum/
  mod.rs
  coordinator.rs
  partition_work.rs
  batch_sink.rs
  ntfs/
    mod.rs
    mft_scan.rs
    path_reconstruction.rs
```

Import pipeline target structure:

```text
import_pipeline/
  mod.rs
  execute.rs
  context.rs
  phases/
    register.rs
    probe.rs
    enumerate.rs
    merge.rs
    analyze.rs
    finalize.rs
  partition/
    candidates.rs
    work.rs
    status.rs
```

Tasks:

- Isolate coordinator state from filesystem-specific enumeration.
- Keep E01 I/O serial and CPU-only transformations parallel.
- Split import phases without changing cancellation, failure, audit, or source
  database state transitions.
- Preserve LVM/XFS/EXT4 behavior and source-local database writes.

### Phase 4.3: File Service

Target structure:

```text
file_service/
  mod.rs
  browse/
    children.rs
    rows.rs
    tree.rs
    jump.rs
  metadata/
    lookup.rs
    sorting.rs
    source_routing.rs
  viewer/
    descriptor.rs
    range.rs
    text.rs
    image.rs
    media.rs
  extraction/
    file.rs
    destination.rs
```

Tasks:

- Separate browsing queries from viewer I/O and extraction.
- Keep `FileEntryId` routing as the only evidence lookup input.
- Preserve media protocol, range clamps, chunk caches, and preview behavior.
- Split MFT-specific resolution behind a file metadata/evidence boundary.

### Phase 4.4: Analysis Aggregates

Target families:

- `correlation`: graph construction, rule evaluation, lead aggregation, and
  pagination.
- `graph_service`: node/edge queries, paging, and source aggregation.
- `artifact_service`: query, detail, aggregation, and source routing.
- `entity_resolution`: extraction, merge, relationships, and cross-case match.
- `rule_pack`: parser, validation, execution, and result projection.

Tasks:

- Retain typed errors at each public boundary.
- Move repeated source iteration to the ready-source router.
- Preserve deterministic ordering after parallel collection.
- Keep graph population non-fatal during import.
- Keep `sourceObjectId` correlation semantics unchanged.

### Phase 4.5: Tests and Review

Test placement:

- Public service behavior: `crates/app-services/tests/<capability>.rs`.
- Private unit behavior: `crates/app-services/tests/unit/<capability>/`.
- Shared fixtures: `crates/app-services/tests/support/`.
- No new test bodies under `src/`.

Regression matrix:

| Area | Required behavior |
|---|---|
| Timeline | Stable ordering, paging, source identity, MACB projection |
| Staging | Transactional merge, root folding, rollback, conflict handling |
| Import | Cancellation, failure states, source DB isolation, deterministic counts |
| File service | Tree/list parity, global IDs, preview, media range, extraction safety |
| Analysis | Windows/Linux capability isolation and ready-source routing |
| Correlation | `sourceObjectId`, family derivation, graph paging, non-fatal population |
| Real samples | Windows/Linux dual import and Linux LVM/XFS file preview |

Performance acceptance:

- No more than 10 percent regression against the Stage 2 real-sample baseline.
- No new whole-source materialization where paging or batching already exists.
- SQLite remains single-writer per source database.
- E01 evidence I/O remains bounded and serial unless measurement proves a safe
  alternative.

### Phase 4.6: Residual Debt Closure

The final Stage 4 cleanup applies the same single-responsibility boundary to
the app-services modules that remained on the Stage 7 migration baseline.

Completed capability splits:

- browser extraction: routing facade, SQLite access, profile/time conversion,
  Chromium history/downloads, Firefox history, and browser records;
- registry extraction: hive dispatch, per-hive extractors, transaction-log
  handling, warning governance, and extraction context;
- LVM expansion: discovery, expansion, diagnostics, source identity, and
  internal models;
- governance scoring: release gates, gate status, fixture/benchmark/security
  rules, score contributions, and scorecard projection;
- notebook: DTO conversion, entry, citation, request-filter, and investigation
  step operations;
- report: HTML orchestration, snapshots, output persistence, warnings,
  catalog/history, and raw JSON bundle export;
- function-level orchestration: email extraction, Windows/Linux summaries,
  GPT/MBR probing, filesystem enumeration, import-analysis workers, and step
  replay.

The Stage 4 service guard now rejects any app-services row reintroduced into
the module-size or function-size baselines. It also validates the complete
capability-module wiring manifest, checks the Cargo dependency graph for Tauri
runtime dependencies, masks comments and string literals before behavioral
token checks, and carries an adversarial `-SelfTest`. CPU-only Rayon work
remains allowed outside the explicitly serial evidence-I/O modules. This is
stricter than a monotonic debt rule: Stage 4 application-service debt must
remain zero.

Closure hardening also locks failure semantics that were exposed during the
split review:

- browser SQLite evidence copies are removed after successful parsing, parser
  failure, and database-open failure;
- registry warning governance preserves primary parser failures, redacts source
  paths, deduplicates warnings, and emits the cap marker only when more than 64
  unique warnings exist;
- raw report bundles disclose unreadable files and use temporary-file plus
  atomic-rename writes so failed reads cannot leave unmanifested partial files;
- import-analysis worker statistics and staging ownership are separated from
  worker runtime/coordinator ownership.

Final validation used both private samples in serial order:
`D:\獬豸杯\检材2.E01` and `D:\獬豸杯\检材3.E01`. Windows-first and Linux-first
dual-source imports both passed with independent source databases and platform
classification.

## Commit and Review Policy

- Stage 3 and Stage 4 are separate commits.
- Each stage receives an independent review after implementation.
- Findings are fixed before the stage commit.
- Baseline rows may only shrink or be removed.
- Historical migrations are never edited; schema changes use new migrations.
