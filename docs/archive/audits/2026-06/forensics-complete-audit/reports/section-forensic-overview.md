# Section 8: Deep Dive — Forensic Overview (Backend + Frontend)

> Archived: supporting section for the 2026-06 complete audit package.

## Summary

- **Artifact extraction is plugin-driven**. The `artifacts-core` crate defines `ArtifactExtractor` and `ArtifactSink` traits; `artifacts-windows` implements extractors for browser history, EVTX, jump lists, LNK, prefetch, recycle bin, registry, SRU, and thumbcache.
- **Analysis orchestration lives in `app-services`**. `analysis_service::extraction::run_analysis_extraction` pre-loads registry hives and transaction logs, then dispatches candidates to specialized extractors (browser, email, EVTX, registry) and writes artifacts + timeline events to SQLite.
- **The forensic overview UI is built from three pages**: `CaseOverview` (case metrics and recent tasks), `V3Dashboard` (graph stats, governance snapshot, correlation), and `V3ScoreCards` (shared stat cards).
- **Artifact browsing is family-based**. The `Artifacts` page lists artifact families as tabs and uses `DenseDataTable` for rows; selecting an artifact updates the global selection store so other panes can correlate.
- **Coverage is Windows-primary, with non-Windows crates unintegrated**. `artifacts-linux`, `artifacts-macos`, and the mobile crates (`artifacts-ios`, `artifacts-android`) have parser modules but are not dispatched by `analysis_service::extraction`, so their artifacts do not yet reach the case database or timeline.

## Key Components

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

## How Extraction Works

1. **Candidate discovery**. `evidence_candidates_for_categories` scans the file catalog for paths matching known artifact patterns (e.g., `NTUSER.DAT`, `*.evtx`, `History` SQLite files).
2. **Registry pre-load**. For registry candidates, the extractor reads the full hive (up to `MAX_ANALYSIS_SOURCE_BYTES` = 128 MiB), pre-computes BootKeys from `SYSTEM`, and loads `.LOG1`/`.LOG2` transaction logs.
3. **Extractor dispatch**. Each candidate is matched to extractors by `supports_path`; extractors receive an `ArtifactContext` (file reader + path + ID) and emit artifacts/timeline events via a sink.
4. **Persistence**. Artifacts are written to `artifacts` table with `source_object_id` set to the originating file entry; timeline events are written to `timeline_events`.
5. **Graph population**. After extraction, correlation logic builds graph nodes and edges linking artifacts, timeline events, and files.

## Platform and Container Coverage

| Platform / Container | Crate | Status | Notes |
|---|---|---|---|
| Windows | `artifacts-windows` | Mature | Registry hives (SYSTEM/SOFTWARE/SAM/SECURITY/NTUSER/USRCLASS/Amcache), EVTX (7 channels), Browser, Prefetch, LNK, JumpList, RecycleBin, SRU, Thumbcache |
| Linux | `artifacts-linux` | Partial | systemd journal, `wtmp` login records, bash history, apt/dpkg history, crontab, sudo auth log |
| macOS | `artifacts-macos` | Partial | FSEvents, Launch Services, binary/XML plist, quarantine events, recent items, Spotlight, unified log tracev3 |
| PST / OST / MBOX | `containers-pst` | Integrated | `PstReader`, `OstReader`, and `mbox::parse_mbox` are wired into `analysis_service::extraction::email.rs` |
| iOS / Android | `artifacts-ios` / `artifacts-android` | Placeholders | Modules exist (iOS: backup, calls, contacts, messages, notes, photos, safari; Android: backup, calls, chrome, contacts, sms) but are not integrated into the main `analysis_service` extraction pipeline |

## Highlighted Strengths

1. **Trait-based extractor registry**. New Windows artifact extractors can be added without modifying the core analysis loop; `ExtractorRegistry` discovers them by path pattern.
2. **Registry transaction-log recovery**. The extractor loads `.LOG1`/`.LOG2` and applies them via `txlog.rs` before parsing, improving completeness of volatile registry data.
3. **BootKey caching**. SAM/SECURITY hash decryption reuses the pre-computed BootKey from the same data source rather than re-reading `SYSTEM` for every hive.
4. **Frontend overview is data-driven**. `V3Dashboard` composes five TanStack Query hooks (`useCurrentCase`, `useGraphSnapshot`, `useTimelineEvents`, `useArtifactFamilyCounts`, `useCorrelationSnapshot`, `useV3GovernanceSnapshot`) to build a unified dashboard.
5. **Selection store enables cross-pane correlation**. Clicking a file, artifact, timeline event, or search hit updates `selection-store.ts`; other panes react to the same ID.

## Risks and Issues Found

### 1. Windows artifact maturity dominates the platform claim

The project is described as a cross-platform forensic workbench, but `artifacts-windows` is substantially more mature than `artifacts-linux`, `artifacts-macos`, and mobile crates. The V3/V4 phases added Linux/macOS crates, but real-sample coverage and extractor counts are likely far behind Windows.

### 2. `AnalysisServiceError` typed errors vs extractor `String` returns

The `ArtifactExtractor::run` trait returns `Result<ExtractorReport, String>`. Extractors that hit parsing errors must encode them as strings, losing the typed error taxonomy used elsewhere. This is a known debt carried over from the `Result<String, String>` cleanup.

### 3. Non-Windows artifact crates are not wired into the main extraction pipeline

`analysis_service::extraction` only dispatches the four Windows-centric categories (`Registry`, `BrowserHistory`, `Email`, `EventLogs`). The `artifacts-linux`, `artifacts-macos`, `artifacts-ios`, and `artifacts-android` crates have parser modules but are not invoked by `run_analysis_extraction`, so their output never reaches the database or timeline on a real case.

### 4. No artifact extraction progress granularity

The import pipeline emits coarse progress events, but artifact extraction is largely opaque to the frontend once it starts. A long registry parse can leave the investigator waiting without per-extractor progress.

### 5. Overview pages fetch independently

`V3Dashboard` fires six parallel queries but has no single snapshot command. If one query fails, the dashboard shows partial data with an error banner. A single backend "overview snapshot" command would reduce round-trips and ensure consistency.

## Improvement Recommendations

### P0 — Before V2 release

1. **Wire Linux and macOS artifact families into the extraction pipeline** by adding `LinuxArtifacts` and `MacArtifacts` categories to `analysis_service::extraction`, with candidate discovery and per-family extractor dispatch.
2. **Add a real Windows E01 regression test for artifact families** beyond the 7 existing E01 tests, to ensure at least one artifact per major family (Browser, EVTX, Registry, LNK, Prefetch) is extracted and counted.

### P1 — Near-term engineering debt

3. **Change `ArtifactExtractor::run` to return a typed error trait** or per-extractor error enum, instead of `String`, so extraction failures can be classified and surfaced correctly.
4. **Add per-extractor progress events** (e.g., `registry:extract:progress`) so the frontend can show granular status during long analysis runs.
5. **Create a single backend overview snapshot command** that returns case metrics, graph counts, artifact family counts, and governance status in one request.

### P2 — Hardening and polish

6. **Build a cross-platform artifact roadmap**. Document which artifact families are supported on Windows vs Linux vs macOS, and prioritize real-sample validation for Linux/macOS to match the Windows level.
7. **Add artifact extractor regression tests** with small fixture files (e.g., a synthetic `NTUSER.DAT`, a single EVTX) so parser changes can be validated without mounting full disk images.

(End of section)
