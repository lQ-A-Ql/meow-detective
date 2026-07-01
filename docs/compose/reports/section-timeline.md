# Section 7: Deep Dive — Timeline Generation

## Summary

- Timeline generation is a **dual-path pipeline**: (1) filesystem enumeration produces raw MACB events via `timeline::project_file_macb`; (2) artifact extractors emit additional events (prefetch, LNK, browser, EVTX, registry, Recycle Bin) through the `ArtifactSink` interface.
- The `timeline` crate is deliberately minimal: a single `lib.rs` (585 lines) converts `FileEntry` timestamps into `TimelineEvent` domain objects, filtering Unix-epoch sentinels and preserving deleted files' available timestamps.
- `TimelineService` (`crates/app-services/src/timeline_service.rs`) orchestrates two MACB projection strategies: an in-memory Rayon path (`project_and_store_macb`) and a SQL bulk-projection path (`ensure_macb_timeline_projected`), guarded by a `timeline_projection_meta` lock to ensure idempotency.
- The `TimelineEvent` domain model uses `source_object_id` as the primary correlation key; the same UUID links file entries, artifacts, timeline events, and graph nodes/edges.
- The frontend `Timeline` page renders a 60-bucket histogram, a paginated event table, and filter controls for time range and event type; events are fetched via TanStack Query and `apiClient.request`.
- Aggregation (`query_timeline_aggregated`) groups events by `(event_type, description)` into stripes/clusters, but is not exposed in the frontend today.

## 1. TimelineEvent Model and DTO

### Domain entity

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

### DTO

`crates/transport/src/dto/timeline.rs` mirrors the entity for IPC with `#[serde(rename_all = "camelCase")]`. Optional provenance fields are `skip_serializing_if` omitted. Three DTOs support the API:

- `TimelineEventDto` — single event.
- `TimelineClusterDto` — grouped events sharing `(event_type, description)` with `count`, `first_ts`, `last_ts`, and up to 5 sample IDs.
- `TimelineStripeDto` / `TimelineAggregatedDto` — server-side aggregation keyed by `event_type`.

### Frontend TypeScript type

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

## 2. How Timeline Events Are Generated from Artifacts

There are two insertion paths into `timeline_events`.

### 2.1 File MACB projection (filesystem path)

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

### 2.2 Artifact extractor path

Artifact extractors consume `ArtifactContext` and write to `dyn ArtifactSink`. The sink interface (`crates/artifacts-core/src/lib.rs`) supports both `write_artifact` and `write_timeline_event`. `new_timeline_event` fills `source_object_id` from `ctx.file_id`, generating a random UUID for the event ID.

Examples found in the code base:

- **Prefetch** (`crates/artifacts-windows/src/prefetch/parser.rs`): each parsed run time yields a `PROGRAM_EXECUTION` event with the executable name in `attrs`.
- **LNK** (`crates/artifacts-windows/src/lnk/parser.rs`): creation/access/write FILETIMEs produce `LINK_CREATED`, `LINK_ACCESSED`, `LINK_MODIFIED` events.
- **Recycle Bin** (`crates/artifacts-windows/src/recycle_bin/parser.rs`): deleted-file timestamps produce timeline events.
- **Registry** (`crates/artifacts-windows/src/registry/parser.rs`): user-assist timestamps and other dated registry values produce timeline events.
- **Browser** (`crates/app-services/src/analysis_service/extraction/browser.rs`): `BROWSER_VISIT` and `BROWSER_DOWNLOAD` events are created from Chrome/Firefox history and downloads; `source_object_id` is `candidate.file_id` (the browser database file).
- **EVTX** (`crates/app-services/src/analysis_service/extraction/evtx.rs`): Windows event log entries with `SystemTime` are converted to timeline events.

The analysis extraction orchestrator (`crates/app-services/src/analysis_service/extraction/mod.rs::run_analysis_extraction`) collects `timeline_events` from every extractor and inserts them in a single batch via `TimelineRepo::insert_batch_with_case`, making it the single write path for non-MACB artifact events.

## 3. MACB Semantics and Aggregation

### 3.1 MACB semantics

The implementation is straightforward: each non-null filesystem timestamp becomes one event. The code does **not** currently distinguish between classic MACB (`M` = modified, `A` = accessed, `C` = created/MFT changed, `B` = birth) and the NTFS `$STANDARD_INFORMATION` vs. `$FILE_NAME` semantics; it simply maps the four columns from `file_entries` to the four event types. This is a pragmatic, filesystem-agnostic approach but may conflate MACB meanings across different file systems (NTFS, FAT, ext4, XFS, APFS, etc.).

Tests in `crates/timeline/src/lib.rs` cover:
- four events when all timestamps are present;
- zero events when none are present;
- epoch filtering;
- deleted files preserving available timestamps;
- directories treated identically to files;
- deterministic output (excluding random IDs);
- future timestamps accepted.

### 3.2 SQL bulk MACB projection

`ensure_macb_timeline_projected` is the lazy/idempotent entry point used by queries. It creates a `timeline_projection_meta` table and, if not already done, runs `project_macb_timeline_sql`. This path builds deterministic IDs as `macb:{file_id}:{event_type}` and uses `INSERT OR IGNORE` with a deduplication subquery based on `(source_object_id, event_type, ts)`. This prevents duplicate MACB rows even if the function is called again.

After projection, `populate_timeline_event_graph` creates `TimelineEvent` graph nodes and `References` edges from each event back to its `source_object_id`.

### 3.3 Aggregation

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

## 4. sourceObjectId Correlation with Artifacts

`source_object_id` is the investigation bridge. The same UUID is used for:
- `file_entries.id` (the originating file);
- `timeline_events.source_object_id` (MACB and artifact-derived events);
- `artifacts.source_object_id` (the file the artifact was extracted from);
- `graph_nodes.id` for `NodeType::TimelineEvent` and `target_id` for `References` edges.

This is the primary mechanism by which the frontend lets an investigator jump from a timeline event to its source. In `Timeline.tsx`, the "跳转到来源对象" button:
- routes to `/artifacts` if `sourceObjectId` starts with `artifact:` (legacy artifact prefix handling);
- otherwise routes to `/files` and sets the selected file ID.

The correlation graph (Section 8) builds `References` edges between timeline events and their source files, enabling artifact-to-timeline leads. However, this relies entirely on extractors setting the field correctly. `AGENTS.md` Gotcha #15 explicitly warns: *"Every new artifact extractor must set this field or cross-artifact leads will silently miss connections."* The warning is not enforced at compile time or by a runtime validator today.

## 5. Persistence Schema

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

## 6. Frontend Timeline Page

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

## 7. Strengths

1. **Clear dual-path design**: filesystem MACB and artifact extractors both feed a single `timeline_events` table, giving a unified view of file, program, browser, and log activity.
2. **Idempotent MACB projection**: `timeline_projection_meta` and deterministic `macb:{file_id}:{event_type}` IDs prevent duplicate events on re-analysis.
3. **Parallel event generation**: `project_and_store_macb` uses Rayon (`par_iter().flat_map_iter`) for CPU-bound MACB projection from large file catalogs.
4. **Correlation-first model**: `source_object_id` ties file entries, timeline events, artifacts, and graph nodes together, enabling cross-artifact investigation.
5. **Typed service errors**: `TimelineServiceError` uses `thiserror` and maps to `ApiErrorDto` categories (`Db`, `NotFound`, `InvalidInput`, `Other`).
6. **Comprehensive backend tests**: `crates/timeline` tests cover MACB semantics, edge cases (epoch, future, deleted), and batch behavior; `timeline_service` tests cover idempotent projection, filtered queries, and aggregation performance up to 10,000 events.
7. **Server-side aggregation capability**: the grouped-by-description stripe API is ready for future UI features (density overview, heat map).

## 8. Risks and Issues Found

### 8.1 MACB projection is filesystem-agnostic and can be semantically lossy

The `FILE_METADATA_CHANGED` event maps to the `changed_at` column from whatever file system parser produced the entry, but this column's meaning varies across NTFS, FAT, ext4, etc. There is no per-filesystem attribution to help investigators interpret `C` vs `B` semantics.

### 8.2 `timeline` crate is a single-file module

At 585 lines it is still within the V3 1500-line limit, but as registry, EVTX, browser, and Recycle Bin projection logic grows, the crate will outgrow a single file. There is no `timeline/projections/` structure today.

### 8.3 No central timeline event factory or validator

Extractors create `TimelineEvent` via `new_timeline_event` (artifact-core) or `make_timeline_event` (analysis service). Titles, descriptions, `confidence`, and `parser_version` are set independently. A malformed extractor could omit `source_object_id`, use an invalid `event_type`, or produce a non-RFC3339 timestamp, and the code does not validate before insertion.

### 8.4 Graph population is non-fatal and silent

`populate_timeline_event_graph` is called with `let _ = ...` in `ensure_macb_timeline_projected`. If graph writes fail, the timeline still exists but the correlation graph is incomplete, with no user-visible warning.

### 8.5 Frontend date handling is brittle

`Timeline.tsx` uses `new Date(timeStart).toISOString()` and `Date.parse(e.ts)` for filters and the histogram. Invalid input is silently ignored or produces NaN buckets. There is no `try/catch` around `new Date()` and no validation message for the user.

### 8.6 Zoom/granularity controls are UI-only

The Zoom In/Zoom Out buttons and the "自适应" label do not actually change bucket count or query resolution. The histogram is always 60 buckets and the query limit is always 100.

### 8.7 Event type filter is page-local

The `<select>` is populated only from events returned in the current 100-row page. Investigators cannot filter by an event type that happens to fall outside the most recent 100 events.

### 8.8 Aggregation endpoint is not consumed

`query_timeline_aggregated` is implemented and tested, but no frontend command or page calls it, so the server-side grouping capability is dead code from the UI perspective.

## 9. Improvement Recommendations

### P0 — Before V2 release

1. **Add a `source_object_id` enforcement test** for every artifact extractor. Create a test harness that runs each extractor against a representative fixture and asserts that at least one emitted timeline event or artifact carries a non-empty `source_object_id` equal to the input file ID. This operationalizes `AGENTS.md` Gotcha #15.
2. **Surface graph population failures** to the user. Change `ensure_macb_timeline_projected` to return a warning or emit a job event when `populate_timeline_event_graph` fails, so investigators know correlation edges may be incomplete.
3. **Validate frontend date inputs**. Replace silent `NaN` handling with explicit error messages and disable the Apply/Clear actions when the datetime-local value is invalid.
4. **Wire or remove zoom/granularity controls**. Either implement bucket-count adjustment (e.g., 30/60/120/240) or hide the buttons and the "自适应" label until the feature is implemented.

### P1 — Near-term engineering debt

5. **Split the `timeline` crate** into `timeline/projections/{macb,artifact,registry,evtx,browser}.rs` and a shared `event.rs` for `TimelineEvent` construction helpers. Keep `project_file_macb` in the MACB module.
6. **Introduce a timeline event schema validator** used by both `TimelineRepo::insert_batch` and `flush_worker_rows`. Reject events with empty `source_object_id`, unknown `event_type`, or unparseable timestamps before insertion.
7. **Normalize event type taxonomy**. Define a `TimelineEventType` enum or constant registry for MACB (`FILE_CREATED`, `FILE_MODIFIED`, `FILE_ACCESSED`, `FILE_METADATA_CHANGED`) and artifact-derived types (`PROGRAM_EXECUTION`, `BROWSER_VISIT`, `BROWSER_DOWNLOAD`, `EVTX_EVENT`, etc.) to prevent typographic drift and enable reliable filtering.
8. **Add a per-filesystem source attribution** to MACB events. Extend `source_attribution` to indicate the origin column and file system parser (e.g., `"ntfs:$STANDARD_INFORMATION:modified_at"`), so investigators can interpret MACB semantics correctly.
9. **Consume the aggregation endpoint** in the frontend. Add a summary/overview mode to the Timeline page that uses `TimelineAggregatedDto` to show stripe counts and clusters before the investigator drills into the raw event list.

### P2 — Hardening and polish

10. **Add a timeline density regression test** against a real E01 fixture with known MACB counts, so changes to filesystem parsers or the MACB projection do not silently alter event counts.
11. **Implement cursor/keyset pagination** for the event table. The current `OFFSET` pagination is fine for tens of thousands of rows but will degrade as cases grow to millions of events.
12. **Document the correlation bridge** for investigators and new developers: a short runbook explaining how `source_object_id` links file entries, timeline events, artifacts, and graph nodes, and what to check when leads are missing.
13. **Consider time-zone handling**. The frontend formats timestamps with `toLocaleString('zh-CN')`, which may surprise non-Chinese users. Allow locale configuration or use ISO UTC display consistently.

## 10. Code Pointers

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
