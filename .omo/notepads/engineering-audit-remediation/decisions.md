# Decisions

## 2026-06-04 Start Work
- Active plan: `.omo/plans/engineering-audit-remediation.md`.
- User selected Start Work; continuation directive says proceed without asking.
- Execute Wave 1 first: tasks 1-5 are independent baselines.

## 2026-06-04 Task 3 MCP Contract Baseline
- Kept MCP protocol redesign out of scope and added baseline tests around current behavior instead of changing DTO casing.
- Treated Tauri top-level command args and nested serde DTO payloads separately: frontend top-level command args remain camelCase while nested `request` DTO fields remain snake_case for this baseline.

## 2026-06-04 Task 5 Quality Gate Baseline
- Baseline documentation only; full expensive gates are deferred to Task 19.
- Treat `frontend/` as the required working directory for every `pnpm` gate command.

## 2026-06-05 Task 10 Contract Synchronization Tests
- Kept MCP normalization out of scope and added a narrower transport test for `mime_type`/`input_schema` snake_case protocol DTO fields, preserving Task 14 ownership of broad normalization.
- Report contract tests remain limited to `ReportHistoryItemDto` because that is the Rust DTO surface available in `crates/transport/src/dto/reports.rs`; frontend-only export fields were not invented.

## 2026-06-05 Task 12 Import Worker/Staging Boundary Extraction
- Kept the new boundary in `import_analysis.rs` rather than creating a third module because the existing worker pool and analysis staging merge already live there; the desktop layer now only builds `PostImportPipelineOptions` and adapts progress to Tauri events.
- Error propagation from the service includes partial `JobOutcomeCounts`; the desktop adapter folds those counts into existing job outcome persistence before returning `CommandError`, preserving cancellation/failure accounting without making app-services depend on job repos or Tauri.

## 2026-06-05 Task 13 MCP API Layer Normalization
- Kept frontend MCP response mapping in the Zustand store because Task 13 only moves backend request construction into the API layer; broad DTO casing normalization remains reserved for Task 14.
- Chose function-level MCP API exports over a client object so store tests can mock each backend operation directly and prove delegation without inspecting raw command payloads in store tests.

## 2026-06-05 Task 14 MCP DTO Casing and Error Compatibility
- Preserved the existing backend MCP protocol wire shape instead of adding Rust serde aliases: nested MCP DTOs stay snake_case for compatibility with current command tests and MCP client types.
- Normalized frontend public MCP API types to camelCase so the Zustand store no longer needs to know backend snake_case response fields except when sending local server config inputs back through API helper functions.

## 2026-06-05 Task 15 DenseDataTable Virtualization Fix
- Kept virtualization local to `frontend/src/components/tables/DenseDataTable.tsx` so all existing page consumers inherit bounded DOM behavior without changing their sort, selection, or empty-state integrations.
- Used fixed-height manual windowing with spacer rows instead of a new dependency or `@tanstack/react-virtual`, matching the existing repo pattern in `HexViewer` and preserving the current semantic table markup.

## 2026-06-05 Task 16 Viewer Large-Content Guardrails
- Kept the existing frontend/backend preview contract unchanged: no new streaming protocol was added because the frontend can satisfy the guardrail requirement by bounding DOM rendering even when preview content is already materialized in memory.
- Matched the repo's current viewer chrome (`bg-[#fafafa]`, thin borders, mono rows, compact toolbar) instead of migrating these components to new semantic tokens during this task, because the requirement was guardrails without viewer redesign.
- Reused the repository's existing fixed-row-height virtualization pattern rather than adding a dependency, with `TextViewer` windowing per 1000-line page and `HexViewer` windowing over raw lines.

## 2026-06-05 Task 17 Timeline Query Index and Pagination Safeguards
- Chose `ORDER BY ts DESC, id ASC` as the repository-wide timeline order because it preserves newest-first semantics while adding a stable primary-key tie-breaker for pagination.
- Added repository-only case-aware filtered helpers instead of changing app-services or DTOs, keeping public timeline behavior unchanged while allowing the persistence layer to test case/type/time query shape directly.
- Added a new additive migration for composite query indexes rather than editing old migration history, so existing databases can upgrade safely.

## 2026-06-05 Task 18 Search Highlight and Indexing Memory Safeguards
- Kept the safeguard at the highlighter boundary instead of changing Tantivy indexing, because indexing semantics and result counts should remain search-engine-owned while returned snippets are the app-facing memory risk.
- Chose a 256 KiB highlighter scan cap to align with `IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES`, plus five snippets per hit and 512 bytes per snippet to bound per-hit response size without changing the existing snippet DTO shape.
