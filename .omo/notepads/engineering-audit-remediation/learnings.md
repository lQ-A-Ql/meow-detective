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

## 2026-06-05 Performance UX Ticket 1 Contract
- Import progress/cancellation contract DTOs are frontend-facing transport types and therefore use `serde(rename_all = "camelCase")`; enum wire values are lowerCamelCase.
- New performance UX event topics intentionally use dotted names (`import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, `performance.report_ready`) and are locked in `EventTopic` tests plus TypeScript `EventTopic` parity.

## 2026-06-05 Performance UX Ticket 2 Import Phase Bridge
- The Tauri import pipeline now treats existing `phase=...` profile strings as the typed event mapping source while preserving legacy `job-progress`: attach→`Attach`, probe/probe-resume/reader-build→`Probe`, enumeration→`Enumerate`, enum-merge→`MergeEnumeration`, analysis/analysis-start→`Analyze`, analysis-merge→`MergeAnalysis`, post-import/post-import-skip/total→`Finalize`.
- Typed `import.phase_progress` metrics remain conservative and parse only known key/value fields from profile details; unknown counts, RSS, workers, and byte totals default to zero/none until richer instrumentation exists.

## 2026-06-05 Performance UX Ticket 3 Import Cancellation State
- Import cancellation state convention: `cancel_import` emits legacy `job-cancelled` plus typed `job.cancellation` as `requested`; pipeline checkpoints emit `acknowledged` when the cancel token is observed, `draining` while enumeration/analysis workers are being allowed to settle, and final background completion emits `cancelled` with `safeToClose=true`.
- Persistence stays migration-free for this ticket: existing `jobs.status` now records `cancelling` during acknowledgement/drain and `cancelled` on terminal cancellation, with details carrying the state reason.

## 2026-06-05 Performance UX Plan Tracking
- Saved the execution tracker at `.omo/plans/performance-ux-optimization.md`, aligned to the current Ticket 1 to 3 worktree progress and the remaining dependency-ordered backend, frontend, and verification tickets.

## 2026-06-05 Performance UX Ticket 4 Partial Results
- Backend partial-result visibility now comes from real import profile milestones: enumeration merge/file-catalog readiness emits bounded FileRows and FileTree metadata, analysis progress emits partial SearchIndex metadata, metadata-only image imports mark TimelineEvents/ArtifactFamily/SearchIndex as deferred, and post-import completion emits ready TimelineEvents/ArtifactFamily/SearchIndex counts parsed from the pipeline summary.
- `import.partial_result` now follows the existing typed event bridge pattern with `EventEnvelope`, `EventTopic::ImportPartialResult`, the exact `import.partial_result` topic, and warn-only emit failures; legacy `job-progress` emission remains in the same profile bridge path.
- Freshness coverage is explicit in tests for ready, partial, deferred, stale, and invalidated states without adding persistence schema or large payloads; `PartialResultDto` still carries only kind, scopeId, readyCount, totalEstimate, queryKey, and freshness.
- Verification: `cargo test -p transport import` passed (11 passed); `cargo test -p forensics-desktop import::pipeline::tests` passed (10 passed, 1 ignored E01 fixture test); `cargo fmt --all -- --check` passed. LSP diagnostics remained unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain.

## 2026-06-05 Performance UX Ticket 5 Scheduling State
- Import analysis profile details now expose bounded scheduling truth through existing typed progress detail strings: `scheduling=queued|running|throttled|deferred|draining`, `workerBudget`, `activeWorkers`, `queuedTasks`, `pendingTasks`, `queueBound`, and metadata-only `contentDeferred`/`textDeferred` flags. No DTO schema, persistence schema, worker count, or scheduler semantics changed.
- App-services progress now emits a scheduled pre-worker profile, richer analysis-start/heartbeat/completion profiles, deferred post-import-skip scheduling metadata, and hard-limit/cancel drain scheduling state so the UI can explain queued, slow/throttled, deferred, and draining/cancelling analysis without a scheduler rewrite.
- Desktop profile parsing now prefers non-zero active workers, falls back to worker budget for deferred/queued profiles, and uses `pendingTasks` as a bounded row-total fallback; tests cover queued budget, deferred metadata-only, throttled memory, and draining cancellation details.
- Verification: `cargo test -p app-services import_analysis` passed (18 passed); `cargo test -p forensics-desktop import::pipeline::tests` passed (12 passed, 1 ignored E01 fixture test); `cargo fmt --all -- --check` passed after applying `cargo fmt --all`. LSP diagnostics remained unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain.

## 2026-06-05 Performance UX Ticket 6 Cache Status
- Backend cache/index observability now emits typed `cache.index_status` events from the same import profile bridge that emits `import.phase_progress` and `import.partial_result`; the event bridge uses `EventEnvelope`, `EventTopic::CacheIndexStatus`, exact topic `cache.index_status`, and warn-only emit failure logging.
- Cache status payloads stay metadata-only and use bounded cache keys for `timeline:events:<dataSourceId>`, `artifacts:family:<dataSourceId>`, and `search:index:<dataSourceId>` with state strings for `warming`, `ready`, `deferred`, `reused`, `stale`, and `invalidated`.
- Import milestones map to honest UI-facing states: analysis-start/heartbeat warms caches, post-import completion marks ready counts, metadata-only/image-backed skip marks deferred, already-merged resume marks reused, merge-in-progress marks stale, and layout changes mark invalidated.
- Verification: `cargo test -p transport import` passed (12 passed); `cargo test -p transport events` passed (3 passed); `cargo test -p forensics-desktop import::pipeline::tests` passed (13 passed, 1 ignored E01 fixture test); `cargo fmt --all -- --check` passed. LSP diagnostics remained unavailable because `rust-analyzer.exe` is missing from the stable Windows toolchain.

## 2026-06-05 Performance UX Ticket 7 Performance Report
- Backend performance report payloads now use `PerformanceReportDto` with a bounded `summary` plus stable numeric metric entries; no raw timeline rows, search hits, file lists, or private paths are included. Locked metric keys include `timeline.query.elapsedMs`, `timeline.query.rows`, `timeline.query.totalRows`, `search.query.elapsedMs`, `search.query.rows`, `search.query.totalRows`, `search.index.elapsedMs`, `search.index.rows`, and optional `*.rowsPerSec`.
- App-services now owns reusable CI-safe timing helpers in `performance.rs`; timeline query and filtered timeline query wrappers prepare report-ready outputs, and search indexing/query wrappers prepare matching report-ready outputs using synthetic or in-memory test seams.
- The desktop event bridge now emits `performance.report_ready` with `EventEnvelope`, `EventTopic::PerformanceReportReady`, exact topic string, and warn-only failure logging. Timeline and search Tauri commands emit the report after successful hot-path calls while preserving response semantics and pagination/order behavior.
- Verification: `lsp_diagnostics` passed with no diagnostics for changed Rust files (`transport/src/dto/import.rs`, `app-services/src/performance.rs`, `timeline_service.rs`, `search_service.rs`, desktop `event_bridge.rs`, `timeline_commands.rs`, and `search_commands.rs`). `cargo test -p transport import` passed (13 passed); `cargo test -p transport events` passed (3 passed); `cargo test -p app-services performance` passed (5 passed); `cargo test -p app-services timeline_service` passed (6 passed); `cargo test -p app-services search_service` passed (2 passed); `cargo test -p forensics-desktop import::pipeline::tests` passed (13 passed, 1 ignored E01 fixture test); `cargo check -p forensics-desktop` passed; `cargo fmt --all -- --check` passed after running `cargo fmt --all`.

## 2026-06-05 Environment LSP Cleanup
- Restored rontend/package.json and rontend/pnpm-lock.yaml after confirming the only manifest-level package addition was the environment-only 	ypescript-language-server dev dependency; the lockfile had been rewritten by the local install and was restored with the manifest.
- Verification: global 	ypescript-language-server --version, pnpm --dir frontend typecheck, empty git diff -- frontend/package.json frontend/pnpm-lock.yaml, and lsp_diagnostics on rontend/src/types/models.ts were run after cleanup.
- Follow-up verification detail: pnpm --dir frontend typecheck currently fails because restored 
ode_modules is missing pre-existing UI dependencies such as eact-day-picker, mbla-carousel-react, echarts, cmdk, aul, eact-hook-form, input-otp, and 
ext-themes; pnpm --dir frontend install --frozen-lockfile also refused because the restored committed lockfile is already out of sync with rontend/package.json. No lockfile-rewriting install was run.

## 2026-06-05 Frontend Dependency Repair
- Added only the direct missing runtime packages proven by frontend source imports: `cmdk@1.1.1`, `embla-carousel-react@8.6.0`, `input-otp@1.4.2`, `next-themes@0.4.6`, `react-day-picker@8.10.1`, `react-hook-form@7.55.0`, `recharts@2.15.2`, and `vaul@1.1.2`.
- Direct import grep did not find source imports for optional candidates such as `@popperjs/core`, `react-popper`, `canvas-confetti`, `react-dnd`, `react-dnd-html5-backend`, `react-responsive-masonry`, `react-slick`, `@mui/material`, `@mui/icons-material`, `@emotion/react`, or `@emotion/styled`, so they were not added.
- Verification passed: `pnpm --dir frontend typecheck`, `pnpm --dir frontend test -- import-contract.test.ts --run`, `lsp_diagnostics` on `frontend/src/types/models.ts`, and `typescript-language-server --version` reported global `5.3.0`.
- Diff review confirmed `frontend/package.json` adds only the eight runtime dependencies, `frontend/pnpm-lock.yaml` updates the lock accordingly and drops stale unmanifested lock entries, and no local `typescript-language-server` dev dependency was added.

## 2026-06-05 Performance UX Ticket 8 Frontend Event UX
- Files changed: `frontend/src/features/jobs/import-event-state.ts`, `frontend/src/features/jobs/import-event-state.test.ts`, `frontend/src/components/layout/TopBar.tsx`, `frontend/src/components/layout/TopBar.test.tsx`, `frontend/src/components/layout/BottomDrawer.tsx`, `frontend/src/components/layout/BottomDrawer.test.tsx`, `.omo/notepads/engineering-audit-remediation/learnings.md`, and `.omo/plans/performance-ux-optimization.md`.
- Ticket 8 now uses a small shared frontend event-state store instead of duplicating subscriptions inside layout components. It listens to `import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, and `performance.report_ready` through the existing event subscriber path, remains mock-safe, and preserves legacy `job-progress`-driven job surfaces.
- Layout UX stayed inside the existing shell: `TopBar` gets compact investigator-facing status chips for import phase, cancellation, partial freshness, cache state, and performance summary, while `BottomDrawer` gets a bounded `Import Signals` block with progress, safe-to-close/draining status, partial result freshness labels, cache reuse/warming labels, and report-ready summary text.
- Verification commands/results: `pnpm --dir frontend typecheck` passed; `pnpm --dir frontend test -- import-contract.test.ts --run` passed (1 file, 5 tests); `pnpm --dir frontend test -- src/features/jobs/import-event-state.test.ts src/components/layout/TopBar.test.tsx src/components/layout/BottomDrawer.test.tsx --run` passed (3 files, 7 tests); `lsp_diagnostics` reported no diagnostics for all changed TS/TSX files.
- Browser smoke check was feasible: `pnpm --dir frontend dev --host 127.0.0.1 --port 4173` was launched and the page was opened through the loaded Playwright wrapper against `http://127.0.0.1:4173` in headed mode. The local wrapper did not persist a screenshot file at the requested path, so the manual artifact path is not recorded even though the shell opened successfully.

## 2026-06-05 Ticket 8 Payload Correction
- Backend Ticket 7 emits `performance.report_ready` as full `PerformanceReportDto { summary, metrics }`, not a bare `PerformanceReportSummaryDto`; the initial Ticket 8 frontend store/tests had accepted only the summary shape and therefore masked the contract mismatch.
- Frontend correction: `frontend/src/types/models.ts` now defines `PerformanceMetric` and `PerformanceReport`; `frontend/src/features/jobs/import-event-state.ts` subscribes to `performance.report_ready` as `PerformanceReport`, keeps the full bounded payload in store, and the layout still reads only `latestReport.summary` so raw metric arrays are not dumped by default.
- Verification passed after the correction: `pnpm --dir frontend typecheck`; `pnpm --dir frontend test -- src/features/jobs/import-event-state.test.ts src/components/layout/TopBar.test.tsx src/components/layout/BottomDrawer.test.tsx --run`; and `lsp_diagnostics` on changed TS/TSX files all returned clean.

## 2026-06-05 Performance UX Ticket 9 Hash Caveats
- Files changed: `frontend/src/features/jobs/import-event-state.ts`, `frontend/src/features/jobs/import-event-state.test.ts`, `frontend/src/components/layout/TopBar.tsx`, `frontend/src/components/layout/TopBar.test.tsx`, `frontend/src/components/layout/BottomDrawer.tsx`, `frontend/src/components/layout/BottomDrawer.test.tsx`, `frontend/src/app/pages/Reports.tsx`, `frontend/src/app/pages/Reports.test.tsx`, `frontend/src/lib/api/mock-data.ts`, `frontend/src/lib/api/mock-provenance.test.ts`, `crates/app-services/src/report_service.rs`, `.omo/plans/performance-ux-optimization.md`, and this notepad.
- Ticket 9 reuses existing typed language instead of adding a broad contract: `PartialResult.kind === evidenceHash` freshness maps to ready/pending/deferred, and data-source `hashStatus` maps to ready/pending/failed/unavailable/deferred caveats. TopBar, BottomDrawer, and Reports page show compact hash labels without displaying source paths.
- Report exports now include bounded evidence-hash warnings in JSON `warnings` and analysis rows for HTML/CSV when source hashes are pending, failed, unavailable, or unknown; warning text reports counts only and avoids raw evidence paths.
- Verification commands/results: `pnpm --dir frontend typecheck` passed; `pnpm --dir frontend test -- src/features/jobs/import-event-state.test.ts src/components/layout/TopBar.test.tsx src/components/layout/BottomDrawer.test.tsx src/app/pages/Reports.test.tsx src/lib/api/mock-provenance.test.ts --run` passed (5 files, 16 tests); `cargo test -p app-services report_service` passed (12 report-service tests); `cargo fmt --all -- --check` passed; `lsp_diagnostics` reported no diagnostics for changed Rust/TS/TSX source and test files.
- Manual QA: `pnpm --dir frontend dev --host 127.0.0.1 --port 4173` plus `pnpm --dir frontend exec playwright screenshot --timeout=30000 http://127.0.0.1:4173/reports C:\Users\QAQ\AppData\Local\Temp\opencode\ticket9-reports.png` captured the mock Reports page showing `Evidence Hash: Unavailable` and the bounded caveat text, with no raw mock source path visible.

## 2026-06-05 Performance UX Ticket 10 Evidence Summary
- Closure scope: Ticket 10 is verification/documentation only. No application code was changed; one stale frontend mock API test assertion was corrected from `hashStatus === 'Recorded'` to the current transport/mock contract value `hashStatus === 'hashed'` after the broader frontend suite exposed it.
- Typed event coverage: Rust transport and frontend parity remain locked for `import.phase_progress`, `import.partial_result`, `job.cancellation`, `cache.index_status`, and `performance.report_ready`. Frontend `import-event-state` subscribes to all five topics; TopBar and BottomDrawer consume the compact state, and the existing Tauri bridge still includes the same topic strings while preserving the legacy `job-progress` bridge.
- UX behavior covered: TopBar shows compact import phase, cancellation/safe-to-close, partial freshness, cache state, performance summary, and evidence-hash status chips. BottomDrawer shows bounded Import Signals with phase/cancellation/partial/cache/report/hash labels. Reports surfaces evidence hash caveats and bounded warning counts without raw source paths; Ticket 9 caveats for pending/unavailable/deferred/failed hash states remain intentional.
- Performance evidence: current evidence is instrumentation and contract coverage, not hardware benchmark numbers. Stable metric keys covered by tests include `timeline.query.elapsedMs`, `timeline.query.rows`, `timeline.query.totalRows`, `search.query.elapsedMs`, `search.query.rows`, `search.query.totalRows`, `search.index.elapsedMs`, `search.index.rows`, and optional `*.rowsPerSec`; elapsed-ms helpers assert non-negative bounded timing rather than machine-specific benchmark targets.
- Targeted Rust gates passed: `cargo fmt --all -- --check`; `cargo test -p transport import` (13 passed); `cargo test -p transport events` (3 passed); `cargo test -p app-services performance` (5 passed); `cargo test -p app-services report_service` (12 passed); `cargo test -p forensics-desktop import::pipeline::tests` (13 passed, 1 ignored real E01 fixture test); `cargo check -p forensics-desktop`.
- Targeted frontend gates passed: `pnpm --dir frontend typecheck`; `pnpm --dir frontend test -- src/types/import-contract.test.ts src/features/jobs/import-event-state.test.ts src/components/layout/TopBar.test.tsx src/components/layout/BottomDrawer.test.tsx src/app/pages/Reports.test.tsx src/lib/api/mock-provenance.test.ts --run` (6 files, 21 tests); `pnpm --dir frontend test -- src/lib/api/case.test.ts --run` after the stale assertion fix (1 file, 5 tests).
- Broader feasible gates passed: `cargo test --workspace` passed, with real/private E01 and fixture-dependent tests remaining ignored/env-gated by design; `pnpm --dir frontend test --run` passed (33 files, 129 tests) after the stale mock assertion fix. The full frontend run still prints expected ErrorBoundary/jsdom console noise from intentional throwing tests, but exits green.
- LSP diagnostics are available and clean now despite older stale issue notes: no diagnostics for `crates/transport/src/dto/import.rs`, `crates/transport/src/events/mod.rs`, `apps/desktop/src-tauri/src/commands/import/pipeline.rs`, `crates/app-services/src/performance.rs`, `crates/app-services/src/report_service.rs`, `frontend/src/types/models.ts`, `frontend/src/features/jobs/import-event-state.ts`, `frontend/src/components/layout/TopBar.tsx`, `frontend/src/components/layout/BottomDrawer.tsx`, `frontend/src/app/pages/Reports.tsx`, and `frontend/src/lib/api/case.test.ts`.
- Known caveats retained: real/private E01 fixtures remain ignored or env-gated and are not default gate requirements; hash unavailable/pending/deferred states are intentionally caveated rather than hidden; performance outputs are bounded instrumentation metrics and elapsed-ms tests rather than real hardware benchmark numbers.
