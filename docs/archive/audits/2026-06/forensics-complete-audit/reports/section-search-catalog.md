# Section 6: Deep Dive — Search and Catalog Indexing

> Archived: supporting section for the 2026-06 complete audit package.

## Summary

- **Full-text search is implemented on top of Tantivy** in `crates/search`. The public surface is small: `search::SearchIndex` wraps the index, `search::extract_text` reads file bytes, and `search::highlight` produces snippets. `crates/search/src/lib.rs:1-9` re-exports these three capabilities.
- **The index schema is intentionally minimal**: `file_id` (STRING|STORED), `path` (TEXT|STORED), `content` (TEXT|STORED), and `name` (TEXT|STORED). `crates/search/src/indexer/tantivy_writer.rs:52-55`.
- **Indexing happens during the artifact-analysis staging merge**, not inside the abstract `ingest` pipeline. Each import worker writes extracted text into a per-worker staging SQLite table; after workers finish, `merge_analysis_staging_to_main` copies those rows into the case Tantivy index at `<case_root>/indexes/tantivy`.
- **Automatic indexing is tightly budgeted**: only files with extensions `txt`, `log`, `csv`, `json`, `xml`, `html`, `htm`, or `md` are considered, each up to 256 KiB, and only the first 100 qualifying files across all workers are indexed. `crates/infrastructure/src/constants.rs:8,11`.
- **The `catalog` crate is dead code**: `crates/catalog/src/lib.rs:1-9` marks it **DEPRECATED** and states it has no production consumers. The in-memory `ExtensionProjection` and `PathPrefixProjection` are not wired into the import or search flows.
- **Frontend Search is a thin query console**: `frontend/src/app/pages/Search.tsx` uses `useSearchResults` (TanStack Query) to call `searchFiles`; the default placeholder query is SQL-like, but the backend is a raw Tantivy `QueryParser`.

## Key Components

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

## How Indexing Works: Ingest-Time vs Query-Time

### Ingest-time pipeline

The `crates/ingest` crate defines an abstract `IngestPipeline` and `IngestSink` but does **not** itself build the search index. The actual indexing path lives in `crates/app-services/src/import_analysis/`:

1. **Worker extraction** (`worker_runtime.rs:180-208`): For every file that passes `should_index_file`, the worker reads the first 256 KiB (`IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES`), calls `search::extract_text`, and, if the result is extractable and non-empty, pushes an `IndexDocRow` into a per-worker staging SQLite table `index_docs`. The global atomic counter `shared.indexed_total` stops the process once 100 files have been indexed (`IMPORT_TEXT_INDEX_LIMIT`).
2. **Staging flush** (`worker_runtime.rs:332-355`): `index_docs` are inserted with `INSERT OR REPLACE` in batches of 25 (`INDEX_DOC_INSERT_BATCH`).
3. **Staging merge** (`analysis_merge.rs:12-96`): After all workers finish, `merge_analysis_staging_to_main` iterates over each worker database. For each worker it first merges artifact/timeline rows into the main case database, then calls `merge_one_analysis_index_docs`.
4. **Index merge** (`analysis_merge.rs:126-172`): Opens or creates the Tantivy index at `<case_root>/indexes/tantivy`, paginates through staging rows 50 at a time (`INDEX_DOC_MERGE_PAGE_SIZE = 50`), builds `search::ExtractedText` values, and calls `SearchIndex::index_documents`.

### Index-time behavior

- `index_documents` deletes any existing document with the same `file_id` before adding the new one (`tantivy_writer.rs:108`), preventing duplicate hits when a file is re-analyzed.
- Binary or empty documents are skipped (`tantivy_writer.rs:110-112`).
- The `name` field is derived from the last path component at index time (`tantivy_writer.rs:114-117`).
- `index_files_chunked` provides per-1000-document commits for long-running indexing, but the production merge path uses the single-commit `index_documents`.

### Query-time pipeline

1. The frontend calls `searchFiles(query, offset, limit)` → `apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, …)` (`frontend/src/lib/api/search.ts:11-16`).
2. The Tauri command `search_files_request` (`search_commands.rs:29-111`) validates the request, checks `MAX_QUERY_LENGTH` (1000 characters), resolves the active case, and verifies the index directory exists.
3. `search_files_real_instrumented` (`search_service.rs:185-202`) opens the Tantivy index and calls `SearchIndex::search`.
4. `SearchIndex::search` (`tantivy_writer.rs:132-201`) parses the query with Tantivy `QueryParser` over the `content` field. If parsing fails, it falls back to a phrase-quoted version of the raw input. It collects `TopDocs` ordered by score plus a `Count` for the total hit count.
5. Snippets are generated by the custom `highlight` function (`highlighter/mod.rs:8-62`), which lowercases both content and query, finds term positions, clusters nearby matches, and returns up to 5 snippets of ≤512 bytes each.
6. Results are mapped to `SearchResultPageDto` (`transport/src/dto/search.rs:26-32`) and emitted to the frontend.

## Search Index Structure and Query Capabilities

### Schema

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

### Query capabilities

- **Default field**: `content`.
- **Syntax**: Tantivy QueryParser syntax (e.g., `keyword`, `"exact phrase"`, `keyword1 AND keyword2`, `content:keyword`).
- **Fallback**: if the parser rejects the input, the whole string is escaped and re-parsed as a phrase query (`"…"`). `tantivy_writer.rs:157-162`.
- **Limits**: `MAX_QUERY_LENGTH = 1000` characters; pagination is capped at `offset + limit ≤ 1000` results (`search_service.rs:135`).

### Highlighting

Highlighting is **not** Tantivy's built-in snippet generator. The custom implementation in `highlighter/mod.rs`:

- Caps scanned content at 256 KiB (`MAX_HIGHLIGHT_CONTENT_BYTES`).
- Splits the query on whitespace, lowercases terms, and finds all positions in the lowercased content.
- Clusters match positions within `SNIPPET_RADIUS * 2 = 120` bytes.
- Returns up to `MAX_SNIPPETS = 5` snippets, each ≤ `MAX_SNIPPET_BYTES = 512` bytes.
- Highlight offsets are byte offsets within the snippet text, not the original document.

This is simple and deterministic, but it does not support phrase queries or proximity matching in snippets, and UTF-16 content is converted to UTF-8 before indexing so offsets are byte offsets in the reconstructed string.

## Catalog Projections (Extension, Path Prefix) and Their Use

The `catalog` crate (`crates/catalog`) provides in-memory projections that are **not currently used** in production. The crate's own `lib.rs:1-9` declares:

> **DEPRECATED**: This crate currently has no consumers in the production codebase. The cataloging functionality has been absorbed into the import pipeline at `apps/desktop/src-tauri/src/commands/import/pipeline.rs`. Retained for reference; scheduled for removal in a future cleanup pass.

### ExtensionProjection

`crates/catalog/src/projection/mod.rs:5-35`:

```rust
pub struct ExtensionProjection {
    index: HashMap<String, Vec<FileEntryId>>,
}
```

- `build(entries)` groups `FileEntryId`s by `entry.ext` (or `""` when no extension).
- `query(ext)` returns `&[FileEntryId]` for the given extension.
- `extensions()` returns all extension keys.

### PathPrefixProjection

`crates/catalog/src/projection/mod.rs:37-75`:

```rust
pub struct PathPrefixProjection {
    index: Vec<(String, Vec<FileEntryId>)>,
}
```

- `build(entries, prefixes)` creates one bucket per requested prefix, then adds any entry whose `path.starts_with(prefix)` to each matching bucket.
- Prefixes are sorted alphabetically in the internal `Vec`.
- `query(prefix)` returns `&[FileEntryId]` for the exact prefix string.

### CatalogIndex

`crates/catalog/src/indexing/mod.rs:6-54` wraps `ExtensionProjection` and stores `total_entries`. It also offers `build_with_prefixes(entries, prefixes)` to return both a `CatalogIndex` and a `PathPrefixProjection`.

### Production status

- No crate declares `catalog` as a dependency.
- No Tauri command, service, or import step calls `CatalogIndex`, `ExtensionProjection`, or `PathPrefixProjection` outside the catalog crate's own tests.
- The catalog's functionality is conceptually replaced by the SQLite `file_entries` table and the `catalog` extension/path-prefix views that the file browser queries directly from the database.

## Frontend Search UI and Hook Integration

### Hook

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

### API layer

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

### Page

`frontend/src/app/pages/Search.tsx:19-252` provides:

- A query input with placeholder `files WHERE extension IN ('.doc', '.xls') AND size > 10MB` and an "执行" button.
- Saved-query management (save, load, delete) persisted to local storage via `@/lib/saved-queries`.
- A `DenseDataTable` showing `path`, `score`, and the first snippet for each hit.
- An inspector pane for the selected hit, including a button to open the file in the file browser.
- Score filtering UI (`highScoreHits` is the count of items with `score >= 0.8`).

### Type mismatch

The UI placeholder suggests a SQL-like query language, but the backend is a raw Tantivy QueryParser. There is no query translation layer; whatever the user types is passed directly to Tantivy. This is a user-experience risk.

## Performance and Correctness Notes

### Performance

- **Index writer buffer**: 15 MB (`tantivy_writer.rs:77`).
- **Chunk commit interval**: 1000 documents (`CHUNK_COMMIT_INTERVAL = 1000`), but production merge uses `index_documents` with a single commit per 50-row page.
- **Benchmark thresholds**: `docs/benchmark-baseline.md:98-102` sets medium search hot query p95 ≤ 1.5 s and large search hot query p95 ≤ 4 s. These are governance thresholds; actual benchmark data is stored in `testdata/governance/v2-benchmark-baseline.json`.
- **Instrumentation**: both `index_files_instrumented` and `search_files_real_instrumented` emit `PerformanceReportDto` metrics (`search_service.rs:105-124` and `185-202`), which the command emits via `event_bridge::emit_performance_report_ready` and records as an investigation step for provenance.

### Correctness

- **Document replacement**: `index_documents` deletes by `file_id` before re-adding, so re-indexing a file does not produce duplicates. `tantivy_writer.rs:526-560` tests this.
- **Incremental indexing**: `index_files_incremental` skips `file_id`s already present in the index, but the production merge path does not use it.
- **Partial index visibility**: `index_documents` commits after each call, so a multi-batch merge produces partially searchable results. `tantivy_writer.rs:705-757` tests this.
- **Highlight correctness**: The custom highlighter is case-insensitive and UTF-8-safe (`floor_char_boundary` ensures no multi-byte character is split), but it does not tokenize; it matches substrings.
- **Text extraction limits**: 10 MiB cap per file, UTF-16 BOM detection, and binary skip based on a conservative MIME-type allow-list. No extraction of PDF, Office, email bodies, or archives.

## Strengths

1. **Typed errors in search**. `IndexError` uses `thiserror` and covers Tantivy, IO, query, and schema errors (`tantivy_writer.rs:13-25`).
2. **Document replacement before indexing**. Prevents duplicate search hits when a file is re-analyzed.
3. **Incremental index reopening**. `SearchIndex::create` calls `Index::open_or_create`, so the same directory can be appended across multiple merge runs.
4. **Performance instrumentation**. Both indexing and search produce structured `PerformanceReportDto` metrics.
5. **Provenance integration**. Every search is recorded as an investigation step with query, offset, limit, and total hits (`search_commands.rs:86-105`).
6. **Custom highlighter is deterministic and safe**. No unsafe slicing, UTF-8 boundaries are respected, and it handles large repeated content gracefully.

## Risks and Issues Found

### 1. Search indexing is coupled to artifact analysis staging

A user cannot search file contents immediately after importing a disk image; search results only appear after the artifact analysis phase has run and its staging DB has been merged. The abstract `ingest` crate (`crates/ingest`) has no search/catalog awareness, so the indexing step is a side-effect of the analysis subsystem rather than a first-class post-import operation.

### 2. `catalog` crate is dead code

`crates/catalog/src/lib.rs` is marked **DEPRECATED** and has no production consumers. Per project rules, dead code should be removed; this crate is a cleanup candidate.

### 3. Limited text extraction coverage

`extract_text` only handles plain text and UTF-16 text files. It does not extract text from PDF, Microsoft Office, PST/OST email bodies, or archives. As a result, the search index covers only a small fraction of the evidence set, and the default placeholder query (`files WHERE extension IN ('.doc', '.xls')`) advertises capabilities that are not actually indexed.

### 4. SQL-like query hint is misleading

The Search page placeholder suggests a SQL-like query language, but the backend uses raw Tantivy `QueryParser`. Users may enter unsupported syntax and get confusing results or zero hits.

### 5. No index lifecycle management

The Tantivy index directory is tied to the case root (`<case_root>/indexes/tantivy`), but there is no explicit cleanup, rebuild, or integrity-check API. If the index becomes corrupted, there is no documented recovery path.

### 6. Search result pagination is capped at 1000 total hits

`search_service.rs:135` computes `search_limit = (offset + limit).min(1000)`. The frontend currently always requests the first page, so this is not yet visible, but it limits deep pagination and any future "show all results" feature.

### 7. Highlight offsets are snippet-local, not document-local

`SearchHighlightDto` offsets are byte offsets within the snippet text, not within the original document. This is acceptable for the current UI but may confuse consumers expecting global offsets.

## Improvement Recommendations

### P0 — Before V2 release

1. **Clarify the search query language in the UI**. Replace the SQL-like placeholder with a Tantivy query example (e.g., `"content:keyword AND name:invoice"`) or add a small syntax help panel. The current placeholder advertises unsupported functionality.
2. **Remove or deprecate the `catalog` crate** from the workspace if it truly has no consumers, to reduce build, dependency, and cognitive surface.
3. **Document the index coverage limits**. The UI and user manual should clearly state that full-text search only covers plain-text files (`.txt`, `.log`, `.csv`, `.json`, `.xml`, `.html`, `.htm`, `.md`) up to 256 KiB and limited to the first 100 qualifying files per import.

### P1 — Near-term engineering debt

4. **Decouple full-text indexing from artifact analysis**. Add a lightweight post-import indexing step that extracts text from all plain-text files in the file catalog, so search is useful even before artifact analysis runs. This would also make the 100-file limit a soft cap rather than an artifact-analysis side effect.
5. **Add index health/rebuild API**. Expose a command to check index integrity and rebuild it from the case database, surfaced in the Settings or CaseOverview page.
6. **Expand extraction coverage**. Integrate a content-extraction pipeline for common formats (at minimum PDF and Office documents) or clearly document the limitation. Until then, consider removing the `.doc` / `.xls` placeholder.
7. **Add a dedicated `search` command test in the Tauri command suite** that exercises the query-length validation and the "no active case" empty-result path.

### P2 — Hardening and polish

8. **Add search result regression tests** with a small fixture index and a known set of queries, so Tantivy upgrades do not silently break query behavior or scoring.
9. **Consider moving the search index into a sub-directory of the case cache** rather than the case root, and include it in backup/restore planning.
10. **Evaluate replacing the custom highlighter** with Tantivy's snippet generator or a token-aware highlighter to support phrase queries and document-global offsets.
11. **Raise or make configurable the 1000-result pagination cap** once deep pagination is needed.

(End of section)
