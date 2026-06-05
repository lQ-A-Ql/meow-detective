# Learnings

## 2026-06-04 Wave 1 Exploration
- Import characterization: best initial test location is `apps/desktop/src-tauri/src/commands/import/pipeline.rs` inline tests because private seams like `execute_import_job` and post-import flag selection are reachable there. Existing logical import test around line 1486 and ignored E01 test around line 1546 are the closest patterns.
- Image-backed behavior: CI-safe full image import is blocked by lack of tiny mountable RAW/E01 fixture. Use existing lazy projection seam in `crates/app-services/src/timeline_service.rs` for non-ignored characterization, and keep full E01 test ignored/manual unless fixture is added.
- Frontend performance tests: add `DenseDataTable.test.tsx` and `VirtualFileTree.test.tsx`; edit `HexViewer.test.tsx` and `TextViewer.test.tsx`. Vitest/jsdom is sufficient; Playwright not needed for baseline DOM-count assertions.
- DenseDataTable currently maps all rows (`rows.map`) and should fail a bounded-DOM expectation until implementation in later task.
- TextViewer already paginates to 1000 lines/page; HexViewer and VirtualFileTree already use visible-window patterns that can be protected by tests.
- DTO contract pattern: transport DTO tests live inline under `#[cfg(test)]` and use `serde_json::to_value` to assert camelCase and absence of snake_case.
- MCP risk: `crates/transport/src/dto/mcp.rs` lacks `serde(rename_all = "camelCase")`; frontend `mcp-store.ts` keeps local snake_case interfaces and sends likely mismatched top-level payload keys such as `serverId` vs Rust `server_id`.
- Official Tauri 2 docs: command args are camelCase by default from TS; Rust command can opt into `rename_all = "snake_case"`, but repo convention favors camelCase DTO/request structs.

## 2026-06-04 Task 1 Import Characterization
- Added inline import-pipeline characterization in `apps/desktop/src-tauri/src/commands/import/pipeline.rs`: logical directory import now asserts the data source row, file row count, eager timeline projection, search index hit, and Prefetch artifact family.
- Added CI-safe image-backed characterization via a pure Raw data-source/file-row seam: metadata-only post-import keeps timeline events at zero, reports deferred timeline/index/artifact output, and `query_timeline` lazily projects four MACB events idempotently.

## 2026-06-04 Task 4 Frontend Large-Data Baselines
- Added large-data baselines in `DenseDataTable.test.tsx`, `VirtualFileTree.test.tsx`, `HexViewer.test.tsx`, and `TextViewer.test.tsx` using DOM-count assertions rather than visual-only checks.
- `DenseDataTable` baseline intentionally documents current unbounded rendering by asserting all 10,000 body rows are present; this should flip only when Task 15 introduces bounded rendering.
- `VirtualFileTree` needed a deterministic `@tanstack/react-virtual` mock in jsdom because the real virtualizer produced no rendered rows without browser layout measurement; the mock still protects the bounded-window contract.
- `VirtualFileTree` baseline now uses a 10,000-node input while still asserting a bounded rendered window from the mocked virtualizer.
- `HexViewer` baseline now asserts a visible-window subset before and after scroll, and `TextViewer` baseline asserts the existing 1000-line page cap and page navigation.

## 2026-06-04 Task 3 MCP Contract Baseline
- Added transport MCP DTO serialization/deserialization baseline tests in `crates/transport/src/dto/mcp.rs`; current MCP DTOs intentionally serialize snake_case response fields and accept snake_case nested request DTO fields.
- Added `frontend/src/stores/mcp-store.test.ts`; it locks the store's current snake_case response mappers and mixed command payload assumptions: camelCase Tauri top-level args (`serverId`, `promptName`) plus snake_case nested `request` DTO fields (`server_id`, `tool_name`, `transport_type`).
- `McpToolCallRequest` currently rejects camelCase `serverId`/`toolName` when deserialized directly through serde, confirming that any future DTO normalization needs explicit aliases or coordinated frontend payload changes.

## 2026-06-04 Task 2 Provenance Contract Baseline
- Added transport inline contract coverage for current analysis provenance: source attribution remains camelCase via `dataSourceId`, `artifactPath`, parser id, category `confidence`, and evidence `sources` without introducing DataSource/Artifact/Timeline schema fields.
- Added an ignored failing-first future contract test for bounded provenance additions: `sourceHash`, `parserVersion`, `confidence`, and `sourceAttribution`; running it with `--ignored` fails at missing `sourceHash`, as expected until migration tasks add the schema.

## 2026-06-04 Task 5 Quality Gate Baseline
- README/AGENTS confirm root Rust gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- `frontend/package.json` confirms Task 19 frontend gates exist: `pnpm typecheck`, `pnpm lint`, `pnpm test`, and `pnpm build`, all to be run from `frontend/`.
- Useful Wave 1 targeted checks documented for later: `cargo test -p transport`, `cargo test -p forensics-desktop import::pipeline::tests`, and `pnpm test -- <test-file>` from `frontend/`.

## 2026-06-04 Task 6 DataSource Provenance Schema
- DataSource provenance is now persisted through `0019_data_source_provenance.sql` and `DataSourceRepo`: `source_hash_sha256`, `hash_status`, `canonical_source_path`, `evidence_size`, `reader_kind`, `provenance_status`, and JSON `provenance_warnings`.
- Legacy or null provenance rows load as `DataSourceProvenance::unknown()` with empty warnings; invalid/unknown status strings also degrade to `Unknown`.
- Affected `DataSource` constructors in app-services and desktop test helpers require explicit `DataSourceProvenance::unknown()` defaults until import code records real provenance.
- Follow-up fix: `attach_data_source` now records bounded source metadata at creation time instead of storing unknown provenance. Files get `hash_status=Pending` and `evidence_size=metadata.len()`; directories get `hash_status=Unavailable` and no size. Both use `DataSourceKind::to_string()` as `reader_kind`, best-effort canonical path, `Recorded` when canonicalization and metadata succeed, and bounded warning strings on best-effort failures.
- Added app-services inline tests for temp file and temp directory attachment to verify persisted provenance through `DataSourceRepo`.

## 2026-06-04 Task 7 Artifact and Timeline Provenance Fields
- Artifact provenance now uses nullable bounded columns and DTO fields: `extractor_id`/`extractorId`, `extractor_version`/`extractorVersion`, `confidence`, and `source_attribution`/`sourceAttribution`; legacy null rows load as `None` and keep `source_object_id` plus `attrs` unchanged.
- Timeline provenance now uses nullable bounded columns and DTO fields: `parser_id`/`parserId`, `parser_version`/`parserVersion`, `confidence`, and `source_attribution`/`sourceAttribution`; missing confidence is valid and loads as `None`.
- Migration `0020_artifact_timeline_provenance.sql` adds nullable provenance columns to existing artifact/timeline tables; fresh schemas stay based on original migrations plus runner application of 0020.
- Analysis staging DBs also gain idempotent nullable provenance-column upgrades so in-progress worker DBs from older layouts can be reopened safely.

## 2026-06-04 Task 8 Report Provenance Propagation
- Report service now exports Task 7 artifact provenance in JSON artifact rows and CSV columns: `extractorId`, `extractorVersion`, `confidence`, and `sourceAttribution`; legacy null values serialize as JSON null and blank CSV cells.
- Report service now exports Task 7 timeline provenance in JSON timeline rows and HTML artifact/timeline summary strings: `parserId`, `parserVersion`, `confidence`, and `sourceAttribution`; incomplete legacy timeline rows render as `unknown` in HTML and null in JSON.
- HTML report still keeps the existing single-column Artifacts/Analysis Provenance shape, but the artifact section now includes bounded provenance text for artifact and timeline rows when full timeline scope is enabled.

## 2026-06-04 Task 9 Frontend Types, Mock Data, and Mock-Mode Labeling
- Frontend DTO parity now includes optional camelCase provenance fields on `DataSourceSummary` (`sourceHash`, `hashStatus`, `canonicalPath`, `evidenceSize`, `readerKind`, `provenanceStatus`, `warnings`), `ArtifactRow` (`extractorId`, `extractorVersion`, `confidence`, `sourceAttribution`), and `TimelineEventDto` (`parserId`, `parserVersion`, `confidence`, `sourceAttribution`).
- The bounded shell location for visible mock-mode labeling is `frontend/src/components/layout/TopBar.tsx`; the badge uses readable text plus `role="status"` so tests can assert it without relying on color.
- Mock forensic-looking records should carry explicit content-level labels such as `[MOCK]`, `MOCK SOURCE`, or `MOCK TIMELINE` rather than relying only on a global app banner.

## 2026-06-05 Task 10 Contract Synchronization Tests
- Transport DTO contract coverage now explicitly includes DataSource provenance camelCase, Artifact/Timeline optional provenance camelCase, bounded Analysis provenance, Report history's Rust-backed shape only, and MCP's current nested snake_case exception.
- Frontend mock parity coverage in `frontend/src/lib/api/mock-provenance.test.ts` now checks DataSource, Artifact, Timeline, Analysis, Evidence Classification, and Report History mock payloads for expected camelCase keys and provenance fields.
- The frontend `DataSourceSummary` parity gap is closed by the current Rust `DataSourceSummaryDto`; no additional DTO fields were needed beyond tests locking `sourceHash`, `hashStatus`, `canonicalPath`, `evidenceSize`, `readerKind`, `provenanceStatus`, and `warnings`.

## 2026-06-05 Task 11 Import Seam Extraction
- First bounded import seam now lives in `app_services::import_precheck`: `prepare_import_source_config` / `prepare_import_source_config_from_path` return `ImportSourceConfig` with validated path, derived source name, `DataSourceKind`, and logical/image mode with staging kind.
- Tauri import pipeline now delegates request/path validation, source classification, source name derivation, staging kind selection, and image-backed post-import config decisions to the app-services seam while retaining job creation, events, staging merge, enumeration, cancellation, and DB updates.
- Required verification outputs were captured in `.omo/evidence/task-11-import-seam.txt`: app-services import tests, desktop import pipeline tests, cargo check, and fmt check all passed.

## 2026-06-05 Task 12 Import Worker/Staging Boundary Extraction
- Post-import worker/staging orchestration now lives in `app_services::import_analysis::run_post_import_pipeline_with_counts` using `PostImportPipelineOptions` and an optional `AnalysisProgressCallback`; it returns `JobOutcomeCounts` and a bounded `PostImportPipelineError` carrying partial counts.
- The Tauri import pipeline now adapts the app-service progress callback to `emit_import_profile_progress`, preserving actual Tauri event emission, job updates, outcome count persistence, final import events, and command validation/state ownership in `apps/desktop/src-tauri/src/commands/import/pipeline.rs`.
- Service-level tests cover post-import skip progress, successful worker/staging summary/count preservation, and cancel failure warning/skipped semantics. Required verification outputs were captured in `.omo/evidence/task-12-import-worker-boundary.txt`.

## 2026-06-05 Task 13 MCP API Layer Normalization
- Frontend MCP backend calls now live in `frontend/src/lib/api/mcp.ts`; `mcp-store.ts` delegates to `getMcpConfig`, `saveMcpConfig`, `addMcpServer`, `removeMcpServer`, `connectMcpServer`, `disconnectMcpServer`, `testMcpConnection`, `listMcpResources`, `listMcpTools`, `callMcpTool`, `listMcpPrompts`, and `getMcpPrompt`.
- Payload strategy is explicit: top-level Tauri command args stay camelCase (`serverId`, `promptName`, `config`, `server`, `request`) while nested MCP DTO/protocol objects remain snake_case (`transport_type`, `auto_connect`, `server_id`, `tool_name`, `input_schema`, `mime_type`) until Task 14.
- Store tests now mock the MCP API module rather than `apiClient.request`; API tests own exact command-name and payload-shape assertions, and the malformed config response baseline still surfaces a store error without changing hardening behavior.

## 2026-06-05 Task 14 MCP DTO Casing and Error Compatibility
- Chosen MCP casing strategy: keep Rust/Tauri nested MCP protocol DTOs snake_case (`transport_type`, `auto_connect`, `server_id`, `tool_name`, `mime_type`, `input_schema`) and keep Tauri top-level args camelCase (`serverId`, `promptName`, `config`, `server`, `request`).
- `frontend/src/lib/api/mcp.ts` now owns the explicit snake_case-to-camelCase boundary and exports camelCase safe models to `mcp-store.ts`; the store remains delegated to the API layer rather than raw `apiClient.request`.
- Malformed/unknown MCP config, status, resource, tool, prompt, test-connection, and tool-call responses are normalized to safe fallbacks in the API layer so store/UI refresh paths do not crash.

## 2026-06-05 Task 15 DenseDataTable Virtualization Fix
- `DenseDataTable` now follows the repo's existing no-dependency virtualization style from `HexViewer`: it tracks scrollTop/container height, slices a visible row window, and uses top/bottom spacer rows to preserve scroll range.
- The table keeps its existing table primitives, colors, sort header wiring, row selection state, and row click semantics because virtualization stays entirely inside `DenseDataTable` rather than changing its callers.
- The 10k-row regression is now locked with bounded DOM assertions, scroll-window assertions proving deep rows appear only after scroll, and a harness test showing external sort/filter state still drives the rendered rows correctly.
- Required verification outputs were captured in `.omo/evidence/task-15-dense-table-virtualization.txt`: focused DenseDataTable tests and frontend typecheck passed.

## 2026-06-05 Task 16 Viewer Large-Content Guardrails
- `HexViewer` now keeps DOM usage bounded by parsing only the currently visible line window instead of materializing parsed JSX for every input line; large datasets show an inline large-content status while preserving the existing forensic viewer styling.
- `TextViewer` now combines the existing 1000-line paging with no-dependency row windowing inside each page, so large previews keep the same toolbar/pager/search UI while rendering only visible nearby lines in the DOM.
- The large-content UX is intentionally narrow: `TextViewer` shows `已截断` when the backend preview is truncated and both viewers show a `大内容模式` status when bounded rendering is active, without redesigning the viewers.
- Required verification outputs were captured in `.omo/evidence/task-16-viewer-guardrails.txt`: focused `HexViewer`/`TextViewer` Vitest coverage and `pnpm --dir frontend typecheck` both passed.

## 2026-06-05 Task 17 Timeline Query Index and Pagination Safeguards
- Timeline repository queries previously ordered only by `ts DESC`; Task 17 now locks deterministic ordering with `ts DESC, id ASC` for unfiltered and filtered queries so identical timestamps paginate stably.
- Repository tests now cover a 150-row unfiltered query, case/type/time filtered queries, identical timestamp pagination across offsets, and nullable legacy `ts` rows sorting last while loading as the default timestamp.
- Timeline query indexes now include `ts/id`, `case_id/ts/id`, `event_type/ts/id`, and `case_id/event_type/ts/id` composites through migration `0021_timeline_query_indexes.sql`.
- Required verification outputs were captured in `.omo/evidence/task-17-timeline-query.txt`: focused persistence timeline tests, app-services timeline tests, migration runner test, persistence check, and fmt check passed.

## 2026-06-05 Task 18 Search Highlight and Indexing Memory Safeguards
- Search extraction already caps text reads at 10 MiB, while import analysis only feeds up to `IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES` (256 KiB) per file into the text-index path; the highlighter now matches that app-facing per-file budget for snippet scanning.
- Highlighter output is now bounded to at most five snippets per hit and 512 bytes per snippet; each snippet remains radius-based around matches, preserving existing highlight offsets relative to snippet text.
- Search crate coverage now includes large repeated-content highlighting, dense repeated-match snippet text limits, direct highlighter scan-cap behavior, zero-result behavior, and many-result total-count/limit behavior.

## 2026-06-05 Task 19 Full Workspace Quality Gate Execution
- Full workspace gates passed after three smallest owning-area fixes: Task 17 timeline repo clippy cleanup, Task 15 DenseDataTable `ResizeObserver` jsdom guard, and Task 14 MCP prompt argument DTO lint usage.
- `pnpm --dir frontend test` now passes `30` files and `115` tests; React/jsdom console noise from intentional ErrorBoundary throwing remains visible but non-failing.
- Required evidence was captured in `.omo/evidence/task-19-quality-gates.txt`, including initial failures, final pass statuses, LSP caveats, and ignored E01/manual fixture test caveats.

## 2026-06-05 Task 20 Regression Matrix Synthesis
- The final matrix is strongest when each audit risk row points to both implementation evidence and final gate evidence, rather than citing only the task that changed code.
- The remaining caveats that must stay visible at the end are fixture-dependent E01 and real-sample tests, unavailable LSP diagnostics, and the intentional MCP camelCase plus snake_case boundary.

## 2026-06-05 Liu Yang Real E01 Regression
- Added an ignored Liu Yang real-sample regression gated by `FORENSICS_LIUYANG_E01_FIXTURE`; set optional `FORENSICS_LIUYANG_EXPECTED_PATH` when the local sample's expected path/name fragment differs from the default `刘洋`.
- The Liu Yang diagnostic output test can be run with `$env:FORENSICS_LIUYANG_E01_FIXTURE='E:\pangushi\刘洋\liuyang_pc.E01'; cargo test -p app-services --test e01_liuyang_regression_test -- --ignored --nocapture`; it prints partition candidates, SYSTEM/SOFTWARE fields, System.evtx boot/shutdown samples, MFT counts, and evidence category totals.

## 2026-06-05 Windows Artifact Parser Coverage
- Added default `artifacts-windows` integration parser coverage for EVTX JSON boot/shutdown extraction, JumpList generic no-LNK artifacts, Thumbcache minimal header metadata, and SRU invalid SQLite report semantics using synthetic source-only inputs.
- SRU remains header-level/generic in default coverage: invalid minimal content is expected to return a report error and no artifacts rather than parsing tables without a real SQLite SRU database fixture.
