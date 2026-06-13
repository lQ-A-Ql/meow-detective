# Forensics Workbench V3 Execution Plan

## 0. V3 Baseline (as of 2026-06-13)

V3 builds on a V2 that is ~90% complete (Grade B, 81/100) with all 7 real E01 regression tests passing. V3 assumes the V2 closeout items (automated nightly regression, memory boundary enforcement, full audit log coverage, final RC drill) are complete before V3-1 begins. The V3 scope is forward-looking: it does not re-litigate V2 decisions, nor does it widen the architectural constraints established in V1 and hardened in V2.

### Architectural constraints carried forward

- Windows-primary, desktop-first, single-user
- No HTTP server; frontend-backend IPC exclusively via Tauri commands/events
- `crates/transport` is the sole IPC contract source of truth
- Evidence is read-only; no evidence files are modified by the tool
- Frontend new UI uses or creates public components only
- UTF-8 for all documentation, fixture manifests, expected JSON, benchmark records
- SQL stays in `persistence-sqlite` or lower; Tauri command handlers delegate to services

---

## 1. V3 Goals

V3 is **not** a feature pile-on. It transforms the tool from a "parser collection + correlation overlay" into a **platform for structured, reproducible digital investigation**. The five pillars:

1. **Evidence Graph** — Unify files, artifacts, timeline events, entities (people/accounts/devices), and investigation leads into a single queryable graph model with typed nodes, weighted edges, provenance chains, and structured confidence. The graph becomes the canonical data model consumed by every view and export.

2. **Container & Cross-Platform Coverage** — Move beyond the current EML/EMLX email floor into full PST/OST/mbox parsing. Add Registry transaction log replay for timeline precision. Expand browser support to current Chromium/Firefox versions. Start a disciplined, fixture-driven expansion into Linux artifacts (systemd journals, wtmp/utmp, apt/dpkg history, bash history, cron, audit logs) and macOS artifacts (plists, unified logs, Spotlight, Quarantine, Launch Services). All new parsers enter the trust framework established in V2-1 — no parser ships without a public fixture, expected JSON, and field guarantee levels.

3. **Reproducible Investigation** — Introduce a **Case Notebook** that records investigator observations, tags, annotations, and analytic conclusions. Every notebook entry must be able to **cite** evidence (a file entry, an artifact record, a timeline event, or a correlation lead). Build **Step Replay** so that an investigator can retrace the exact sequence of imports, searches, filters, and correlation queries that produced a finding. The notebook, citations, and replay log become part of the exported report.

4. **Rule Pack System** — The V2 correlation rules are hardcoded in Rust. V3 introduces a **declarative rule pack format** (TOML or YAML, versioned) so that investigation templates and hit-rule packs can be authored, shared, imported, and validated without recompilation. Rule packs carry metadata (author, version, scope, caveats, expected fixtures). Support **org-level verification configurations**: a lab can define a "baseline" rule pack that must pass before a case is considered reviewed. The release scorecard from V2-4 extends to consume rule-pack coverage.

5. **Offline Batch Processing** — For very large cases (e.g., multi-terabyte RAIDs, enterprise NAS images), import and analysis cannot be interactive. V3 introduces a **local batch subsystem**: import plans are queued with phase definitions (mount → catalog → extract artifacts → index → correlate), each phase is recoverable on interruption, progress is persisted across restarts, and the investigator can inspect partial results while later phases run. This remains local/desktop; there is no cloud orchestration or distributed workers.

---

## 2. Key Interfaces & DTOs

V3 introduces or materially expands the following contract areas. All new DTOs live in `crates/transport/src/dto/`, use `#[serde(rename_all = "camelCase")]`, and are mirrored in `frontend/src/types/models.ts`.

### 2.1 Evidence Graph DTOs (new)

| DTO | Purpose |
|-----|---------|
| `GraphNodeDto` | Typed node: File, Artifact, TimelineEvent, Entity, Lead, NotebookEntry. Carries id, type discriminant, label, summary, tags. |
| `GraphEdgeDto` | Typed edge: Contains, References, CorrelatesWith, Cites, Annotates, Precedes, DerivesFrom. Carries source/target node ids, edge type, confidence, rule provenance. |
| `GraphQueryDto` | Traversal query: start node(s), edge type filters, max depth, confidence floor, limit. |
| `GraphQueryResultDto` | Matched subgraph: nodes + edges, summary statistics, query plan metadata. |
| `GraphSnapshotDto` | Full graph projection for a case: node/edge counts by type, graph density, schema version. |

### 2.2 Notebook & Citation DTOs (new)

| DTO | Purpose |
|-----|---------|
| `NotebookEntryDto` | A timestamped observation: free-text notes, structured fields (finding, hypothesis, action item, conclusion), linked evidence citation ids, parent entry id (threading). |
| `EvidenceCitationDto` | A pointer to a specific piece of evidence: citation type (FileEntry/Artifact/TimelineEvent/Lead/NotebookEntry), target id, display label, snippet or offset, timestamp at citation time. |
| `InvestigationStepDto` | A recorded action: step kind (Import, Search, Filter, Correlate, Tag, Annotate, Export), parameters snapshot, timestamp, duration, case state hash before/after. |
| `StepReplayDto` | Ordered list of InvestigationStepDto with replay metadata: total steps, replayable flag, caveats for non-deterministic steps. |
| `NotebookExportDto` | Full notebook serialization for report inclusion: entries, citations, thread graph, investigator identity. |

### 2.3 Rule Pack DTOs (new)

| DTO | Purpose |
|-----|---------|
| `RulePackManifestDto` | Metadata: name, version, author, description, scope (artifact types / OS), expected fixture hashes, minimum product version, caveats. |
| `RuleDefinitionDto` | A single correlation rule: name, description, source node types, target node types, edge type, match conditions (field-level predicates), confidence level, caveats. |
| `RulePackDto` | A deployable unit: manifest + rule definitions + optional expected-output fixtures. |
| `RulePackValidationResultDto` | After loading a pack: parse errors, schema violations, fixture mismatches, coverage overlap with existing packs, confidence calibration warnings. |
| `RulePackCoverageDto` | Aggregate: which rule families are covered by which packs, conflict/overlap report, pack freshness relative to product version. |

### 2.4 Batch Processing DTOs (new)

| DTO | Purpose |
|-----|---------|
| `BatchPlanDto` | Import plan: data source references, phase definitions (Mount, Catalog, ExtractArtifacts, Index, Correlate), phase dependencies, resource limits (max memory, thread count). |
| `BatchPhaseDto` | Single phase status: phase kind, state (Queued/Running/Completed/Failed/Paused), progress 0.0-1.0, elapsed, estimated remaining, error count, warnings. |
| `BatchJobDto` | Top-level batch job: id, label, plan, phases, overall progress, created/started/completed timestamps, restart count, result summary. |
| `BatchResumeDto` | Resume capability: which phases can resume from checkpoint, which must restart, data loss risk assessment. |

### 2.5 Container & Parser Coverage DTOs (expand existing)

| DTO | Purpose |
|-----|---------|
| `EmailMessageDto` (expanded) | Add `container_path` (PST/OST/mbox internal folder path), `message_class` (IPM.Note, etc.), `is_embedded`, `parent_folder_id`, and attachment fidelity flags. |
| `RegistryTransactionDto` (new) | A single transaction log entry: operation (Create/Modify/Delete), key path, value name/data before/after, sequence number, timestamp. |
| `LinuxArtifactDto` (new) | Union/enum covering: systemd journal entries, wtmp/utmp login records, bash history lines, apt/dpkg package events, cron job definitions, sudo logs. |
| `MacArtifactDto` (new) | Union/enum covering: plist key-value entries, unified log entries, Spotlight metadata, Quarantine events, Launch Services database entries, recent items. |
| `ParserSupportMatrixEntryDto` (expanded) | Add `platform` discriminant (Windows/Linux/macOS/Cross), `artifact_family` field, `container_format` for nested parsers (PST inside E01 inside raw). |
| `FixtureManifestDto` (expanded) | Add `platform`, `artifact_family`, `container_chain` fields. |

### 2.6 Documentation entry points

V3 is governed by this plan, with detail carried in:
- `docs/evidence-graph-design.md` — graph schema, query language, indexing strategy
- `docs/case-notebook-design.md` — notebook model, citation linking, step recording
- `docs/rule-pack-spec.md` — rule pack format, validation rules, sharing conventions
- `docs/batch-processing-design.md` — batch subsystem architecture, checkpointing, resource governance
- `docs/linux-artifact-coverage.md` — Linux parser roadmap, fixture requirements, known gaps
- `docs/mac-artifact-coverage.md` — macOS parser roadmap, fixture requirements, known gaps
- `docs/pst-ost-mbox-support.md` — container email roadmap, Outlook/Thunderbird version matrix

Existing docs updated: `docs/parser-support-matrix.md`, `docs/known-unsupported-formats.md`, `docs/error-taxonomy.md`, `docs/validation-trust-framework.md`, `docs/release-scorecard.md`.

---

## 3. Stage Design

### Stage V3-1: Evidence Graph Foundation

#### Objective

Build the unified graph data model that replaces ad-hoc joins between files, artifacts, timeline, and leads with a single, typed, queryable, provenance-carrying graph. All existing frontend views (File Browser, Timeline, Artifacts, CorrelationWorkspace) consume the graph as their canonical data source. The V2 correlation engine becomes a graph-population rule engine.

#### Stage boundaries

**In scope:**
- Graph schema definition: node types (File, Artifact, TimelineEvent, Entity, Lead, NotebookEntry placeholder), edge types (Contains, References, CorrelatesWith, Cites placeholder, Annotates placeholder, Precedes, DerivesFrom)
- Graph population at import time: filesystem tree as Contains edges, artifact extraction creates Artifact nodes + References edges to File nodes, timeline events create TimelineEvent nodes + References edges
- Entity extraction v1: Person (email addresses, usernames), Device (hostname, volume serial, MAC), Account (SID, UID). Entity nodes linked to evidence nodes via DerivesFrom edges.
- Graph persistence: SQLite-backed graph store with indexed adjacency, efficient traversal queries up to depth 4
- Graph query API: start-node traversal, edge-type filtering, confidence floor, depth limit, pagination
- Migration of existing views: FileBrowser, Timeline, Artifacts list, and CorrelationWorkspace all read from the graph projection DTOs; no view queries raw file/artifact/timeline tables directly
- Provenance chain: every graph edge carries source rule/parser, extraction timestamp, parser version
- Graph statistics (node/edge counts by type, density, largest component) as governance signal
- `/v3` governance dashboard foundation: replaces `/v2` with graph-aware signals

**Deferred to later stages:**
- NotebookEntry and Cites/Annotates edge types defined but not populated until V3-3
- Rule pack engine consumes graph edges but rule pack authoring is V3-3
- Entity resolution/deduplication across artifacts is v1 (basic) only; advanced entity merging is V4
- Graph visualization (interactive force-directed/radial) — basic node-edge table views only; full viz is V3-2 frontend polish
- Graph query language (Cypher-like DSL) — V3-1 uses structured Rust API only; DSL is V4
- Incremental graph update on re-import — V3-1 rebuilds graph on fresh import only

#### Phase Tasks

**Phase 1: Graph Schema & Persistence (weeks 1-3)**
1. Define `GraphNode` and `GraphEdge` domain types in `crates/domain/` with typed discriminants, not string tags.
2. Define `GraphNodeDto`, `GraphEdgeDto`, `GraphQueryDto`, `GraphQueryResultDto`, `GraphSnapshotDto` in `crates/transport/src/dto/graph.rs`.
3. Create SQLite graph store in `crates/persistence-sqlite/`: `graph_nodes` table (id, type, label, summary, case_id, created_at), `graph_edges` table (id, source_id, target_id, edge_type, confidence, provenance_json, case_id), indexed on (type), (source_id), (target_id), (source_id, edge_type).
4. Build `graph_repo` with: `insert_nodes_batch`, `insert_edges_batch`, `traverse(start_ids, edge_types, max_depth, confidence_floor, limit) -> (nodes, edges)`, `get_snapshot(case_id) -> GraphSnapshotDto`.
5. Add graph store initialization to `AppState` alongside existing SQLite pool; graph uses the same WAL database (separate tables, same connection pool).
6. Graph migration scripts: ensure forward compatibility for new node/edge types added in later stages.

**Phase 2: Graph Population — Import Pipeline (weeks 3-5)**
1. Extend `IngestPipeline` trait with `populate_graph(&self, graph: &mut dyn GraphWriter)` — each ingest stage writes its nodes/edges.
2. Filesystem ingest: create File nodes, parent→child Contains edges, volume→root Contains edges.
3. Artifact extraction ingest: create Artifact nodes per extracted record, References edges from Artifact→File (for path-based links), Precedes edges between artifacts with temporal ordering.
4. Timeline ingest: create TimelineEvent nodes, References edges to File/Artifact nodes based on shared `sourceObjectId`.
5. Entity extraction ingest v1: regex + structure-based extraction of Person (email pattern, username fields), Device (hostname, volume serial), Account (SID, UID). Create Entity nodes + DerivesFrom edges to the source Artifact/File node.
6. Correlation ingest: the existing `correlation_service` writes CorrelatesWith edges (replacing in-memory correlation result assembly) with confidence, rule provenance, and match signals stored on the edge.
7. Implement `GraphWriter` trait with transaction-batched writes; ensure a failed import rolls back the graph to previous case state.
8. Graph population must not regress import p95 times beyond V2 baseline +15%.

**Phase 3: Query API & Service Layer (weeks 5-7)**
1. Build `graph_service` in `crates/app-services/src/graph_service.rs`: `get_graph_snapshot(case_id)`, `query_graph(query)`, `get_node_neighborhood(node_id, depth)`, `get_provenance_chain(edge_id)`.
2. Tauri commands: `get_graph_snapshot`, `query_graph`, `get_node_neighborhood`, `get_provenance_chain` in `apps/desktop/src-tauri/src/commands/graph_commands.rs`.
3. Register all new commands in `apps/desktop/src-tauri/src/lib.rs` `invoke_handler`.
4. DTO conversions: `From<GraphNode> for GraphNodeDto`, etc. in `crates/app-services/` or a dedicated `graph` conversion module.

**Phase 4: Frontend Migration (weeks 7-9)**
1. Mirror all graph DTOs in `frontend/src/types/models.ts` as `GraphNode`, `GraphEdge`, `GraphQuery`, `GraphQueryResult`, `GraphSnapshot`.
2. API module `frontend/src/lib/api/graph.ts` with `getGraphSnapshot()`, `queryGraph()`, `getNodeNeighborhood()`, `getProvenanceChain()`.
3. Mock data in `frontend/src/lib/api/mock-data.ts` for graph endpoints.
4. React Query hooks in `frontend/src/features/graph/hooks.ts`.
5. Migrate `CorrelationWorkspace` to consume graph data instead of `CorrelationSnapshotDto` (which becomes a typed projection of the graph).
6. Migrate `FileBrowser` to render from graph File nodes + Contains edges (lazy child loading via graph traversal).
7. Migrate `Timeline` page to render TimelineEvent nodes + neighborhood (linked files/artifacts) loaded via `getNodeNeighborhood`.
8. Migrate `Artifacts` page to render Artifact nodes + linked Files/TimelineEvents via neighborhood query.
9. Build `/v3` governance dashboard page: shows graph statistics, node/edge type distribution, largest component size, provenance coverage.
10. Frontend tests: each migrated page passes existing test suites with graph mock data.

**Phase 5: Provenance & Governance Integration (weeks 9-10)**
1. Provenance chain UI: clicking any edge shows source rule/parser, extraction time, parser version.
2. Graph health governance signals: node/edge counts, orphan detection (Artifact nodes with no File edge, File nodes with no parent), type distribution chart.
3. Integrate graph signals into `V3GovernanceSnapshotDto` (supersedes V2's `V2GovernanceSnapshotDto`) with graph-specific `runtimeSignals`.
4. Graph regression tests: for each core fixture, verify expected node/edge counts by type.
5. Benchmark graph population: measure graph write overhead during import for small/medium/large datasets.
6. Documentation: `docs/evidence-graph-design.md` finalized with schema, query patterns, and extension guide.

#### Acceptance Criteria

- All 5 node types (File, Artifact, TimelineEvent, Entity, Lead) and 4+ edge types (Contains, References, CorrelatesWith, DerivesFrom, Precedes) defined and persisted.
- A fresh import of any V2 medium fixture produces a graph with verifiable node/edge counts (automated regression).
- Every existing frontend view (FileBrowser, Timeline, Artifacts, CorrelationWorkspace) renders from graph data with no regression in functionality or performance.
- Graph traversal from any File node reaches its Artifact nodes, TimelineEvent nodes, and Entity nodes within depth <= 3.
- Every CorrelatesWith edge carries provenance: source rule id, extraction timestamp, parser version.
- `/v3` dashboard page loads with graph statistics and governance signals.
- No regression in import p95 time >15% over V2 baseline for medium dataset.
- All existing V2 regression tests still pass (cmd layer, media protocol, SQL boundary, frontend unit tests).

---

### Stage V3-2: Container & Cross-Platform Coverage

#### Objective

Expand evidence ingestion beyond the current EML/EMLX floor and Windows-only artifact scope. Deliver PST/OST/mbox parsing, Registry transaction log replay, current browser version support, and initial disciplined coverage of Linux and macOS artifacts — all following the V2-1 trust framework (public fixtures, expected JSON, field guarantee levels, known-limitation documentation).

#### Stage boundaries

**In scope:**
- **PST/OST/mbox parsing**: read PST (32-bit and 64-bit Unicode), OST, and mbox files. Extract email messages, attachments, folder structure, calendar items, contacts. All extracted items become EmailMessage Artifact nodes in the evidence graph. Nested PST inside E01 inside raw supported.
- **Registry transaction logs**: parse `.LOG1`/`.LOG2` transaction log files alongside existing hive parsing. Expose transaction log entries as `RegistryTransaction` Artifact nodes. Enable timeline precision improvement: use transaction timestamps as exact modification times, not just hive last-modified.
- **Browser history updates**: bring Chrome, Edge, and Firefox parsing current to late-2025/early-2026 schema versions. Add download history, cookie analysis, and session restore file parsing. Move browsers from "Experimental" to "Supported" with >= public-medium fixture for each browser family.
- **Linux artifacts v1**: systemd journal (`/var/log/journal/`), wtmp/utmp login records, bash history (`.bash_history`), apt/dpkg log (`/var/log/apt/`, `/var/log/dpkg.log`), cron (`/var/spool/cron/`, `/etc/crontab`), sudo logs (`/var/log/auth.log`). Minimum: public-small fixture per artifact type, expected JSON, field guarantee levels.
- **macOS artifacts v1**: plist parsing for key forensic plists (`.plist` binary and XML), unified log entries (`/var/db/diagnostics/`), Spotlight metadata (`.store.db` / `.Spotlight-V100/`), Quarantine database (`/Users/*/Library/Preferences/com.apple.LaunchServices.QuarantineEventsV2`), Launch Services, recent items. Minimum: public-small fixture per artifact type.
- **Parser trust integration**: every new parser enters `docs/parser-support-matrix.md` with platform discriminant, artifact family, container chain, sample coverage, field guarantee levels, and known limitations. New error taxonomy entries for container/parse errors.
- **Cross-platform `testdata/fixtures/` reorganization**: add `linux/` and `mac/` subdirectories alongside existing Windows fixtures.
- **Extended `ParserSupportMatrix` DTO**: add `platform`, `artifact_family`, `container_chain` fields.

**Deferred:**
- Full disk image support for Linux filesystems (ext4, XFS, Btrfs) — V3-2 only supports Linux artifacts from imported file trees, not from raw Linux disk images
- macOS APFS/HFS+ filesystem parsing — same as above; artifacts from imported file trees only
- iOS/Android mobile artifacts
- Cloud artifact collection (AWS CloudTrail, Azure Audit, GCP logs)
- Carving/recovery of deleted files from Linux/macOS filesystems
- PST password-protected/encrypted message support (deferred to V4)
- Full PST/OST property-level fidelity — V3-2 targets message/attachment/folder/calendar/contact extraction, not every MAPI property

#### Phase Tasks

**Phase 1: PST/OST/mbox Parsing Crate (weeks 1-4)**
1. Create `crates/containers-pst/` crate: parse PST (Unicode 32/64-bit), OST files. Dependencies evaluated and documented (decision record similar to `docs/evtx-dependency-decision.md`).
2. Implement PST: read header, node BTree, block BTree, Name-to-ID map, property context. Extract: Message (subject, body plain+HTML, sender, recipients, sent/received time, attachments), Folder (name, parent, depth, count), Calendar (subject, start/end, location, attendees), Contact (name, email, phone, address).
3. Implement mbox: RFC 4155 mboxrd/mboxo/mboxcl/mboxcl2 detection, message boundary parsing, From-line escaping. Reuse EML parsing for individual message bodies.
4. Implement OST: share PST code for message/folder extraction; add OST-specific table handling (offline folder tables, synchronization metadata).
5. Container ingestion in `IngestPipeline`: PST/OST/mbox files are detected during file discovery (signature-based). When ingested, the container is opened, folder structure is created as File nodes in the graph, messages are extracted as EmailMessage Artifact nodes with folder-path metadata. Attachments are extracted to the evidence cache and become File nodes with References edges from the EmailMessage.
6. Nested container support: PST within E01 within raw; mbox within tar within E01.
7. PST/OST/mbox fixtures: `public-small` (synthetic single-message PST, 5-message mbox), `public-medium` (multi-folder PST with attachments, 100+ message mbox), `private-real-regression` (real Outlook PST with calendar/contacts).
8. Expected JSON per container format: message fields, attachment linkage, folder path, property fidelity flags.
9. Parser support matrix entry: PST, OST, mbox with version range, known limitations (encryption, property coverage), field guarantee levels.

**Phase 2: Registry Transaction Logs (weeks 4-5)**
1. Extend `crates/artifacts-windows/src/registry/` with transaction log parser: read `.LOG1` and `.LOG2` files adjacent to registry hives.
2. Parse transaction log entries: operation (CreateKey, DeleteKey, SetValue, DeleteValue, RenameKey), key path, value name, value data before/after, sequence number, timestamp.
3. Expose as `RegistryTransaction` Artifact type with associated DTO.
4. Graph integration: RegistryTransaction nodes with References edges to parent Registry hive artifact and related File nodes. Precedes edges for temporal ordering within a hive.
5. Timeline precision: when a RegistryTransaction exists for a key modification, use its timestamp for timeline event precision instead of the coarser hive last-modified time.
6. Fixtures: `public-small` synthetic hive with known transaction log, `public-medium` real hive image with transaction log.
7. Known limitation: transaction log is ring-buffer; older entries may be overwritten. Document detection of truncation.

**Phase 3: Browser Version Refresh (weeks 5-7)**
1. Audit current Chrome/Edge/Firefox parser versions against latest stable browser releases (as of 2026).
2. Update Chrome/Edge History SQLite schema for v130+ schema changes, if any.
3. Update Firefox places.sqlite schema for v130+ changes.
4. Add download history parsing (Chrome: `History` downloads table, Firefox: `places.sqlite` moz_annos and `downloads.json`).
5. Add cookie analysis: Chrome/Edge `Cookies` SQLite, Firefox `cookies.sqlite`. Expose as `BrowserCookie` Artifact type with domain, name, expiry, secure flag, same-site policy.
6. Add session restore parsing: Chrome/Edge `Sessions/` and `Session Storage/`, Firefox `sessionstore-backups/`. Expose tab/window structure at last session.
7. Move browser parsers from `Experimental` to `Supported` in the support matrix: each must have `public-medium` fixture, expected JSON, field guarantees.
8. Create `public-medium` fixtures: Chrome profile directory with history, downloads, cookies from a controlled browsing session; same for Firefox.
9. Update `docs/parser-support-matrix.md` and `docs/known-unsupported-formats.md`.
10. Add browser cookie and session restore as correlation rule sources: cookie domain → BrowserHistory URL, session restore tab URL → BrowserHistory entry.

**Phase 4: Linux Artifacts v1 (weeks 7-9)**
1. Create `crates/artifacts-linux/` crate with artifact family definitions: SystemdJournal, WtmpLogin, BashHistory, AptHistory, DpkgLog, CronJob, SudoLog.
2. Define `LinuxArtifact` domain type (enum per family) and `LinuxArtifactDto` transport type.
3. Systemd journal parser: read binary journal files (`/var/log/journal/<machine-id>/`), extract entries with timestamp, message, PID, UID, GID, executable, priority, boot ID. Handle compressed journals (LZ4, XZ, ZSTD).
4. Wtmp/utmp parser: read `/var/log/wtmp` (binary), `/var/run/utmp` (binary), `/var/log/btmp` (bad logins). Extract: user, terminal, host, login/logout time, PID.
5. Bash history parser: read `~/.bash_history` and `/root/.bash_history`, extract command line, optional timestamp (HISTTIMEFORMAT).
6. Apt/dpkg parser: read `/var/log/apt/history.log`, `/var/log/dpkg.log`. Extract: package name, version, action (install/upgrade/remove), timestamp.
7. Cron parser: read `/var/spool/cron/crontabs/`, `/etc/crontab`, `/etc/cron.d/`, `/etc/cron.{hourly,daily,weekly,monthly}/`. Extract: schedule, user, command.
8. Sudo log parser: read `/var/log/auth.log`, detect sudo session open/close, command, user, timestamp.
9. Ingest integration: Linux artifacts are detected and extracted when a Linux file tree is imported. They become Artifact nodes in the evidence graph with appropriate entity links (User entities from wtmp/bash/sudo).
10. Linux fixtures: `public-small` per artifact type (synthetic), `public-medium` (VM snapshot file tree from a controlled Linux VM session).
11. Ensure each Linux artifact parser enters the trust framework: public fixture, expected JSON, field guarantee levels, known limitations.
12. Multi-OS case support: a case can contain both Windows and Linux artifacts; graph nodes carry `platform` discriminant; cross-platform correlation rules are tagged accordingly.

**Phase 5: macOS Artifacts v1 (weeks 9-11)**
1. Create `crates/artifacts-macos/` crate with artifact family definitions: Plist, UnifiedLog, Spotlight, Quarantine, LaunchServices, RecentItems, FSEvents.
2. Define `MacArtifact` domain type and `MacArtifactDto` transport type.
3. Plist parser: support binary plist (bplist00), XML plist. Key forensic plists: `com.apple.airport.plist` (WiFi networks), `com.apple.dock.plist` (persistent apps), `com.apple.recentitems.plist`, `com.apple.sidebarlists.plist`, `com.apple.LaunchServices.QuarantineEventsV2`, `com.apple.loginitems.plist`, `com.apple.spotlight.Shortcuts`.
4. Unified log parser: read `/var/db/diagnostics/` tracev3 files, extract entries with timestamp, process, message, activity ID, thread ID. Handle log fragmentation and rotation.
5. Spotlight metadata parser: read `.store.db` (Spotlight index SQLite) and `.Spotlight-V100/` store files. Extract file path, display name, kind, content type, dates, authors, etc.
6. Quarantine database parser: read `QuarantineEventsV2` SQLite, extract URL, origin bundle, quarantine agent, timestamp.
7. Launch Services parser: read `com.apple.LaunchServices.plist` and `/var/db/launchd.db/` for service definitions.
8. Recent Items parser: read `com.apple.recentitems.plist` for files, servers, applications.
9. FSEvents parser: read `.fseventsd/` event logs for file system change history on macOS.
10. Ingest integration: same pattern as Linux — detected from imported macOS file tree, become Artifact nodes in graph.
11. macOS fixtures: `public-small` per artifact type, `public-medium` (controlled macOS VM snapshot).
12. Trust framework integration: same as Linux — matrix entry, expected JSON, field guarantees, known limitations.

**Phase 6: Cross-Platform Governance & Integration (weeks 11-12)**
1. Update `/v3` dashboard to show platform coverage: Windows/Linux/macOS artifact family counts, fixture coverage by platform.
2. Extend `ParserSupportMatrixDto` with `platform`, `artifact_family`, `container_chain` fields.
3. Correlation rules tagged with platform compatibility: LNK/Prefetch/Registry → Windows only; bash/wtmp → Linux only; plist/Quarantine → macOS only; browser → cross-platform.
4. Report export: platform breakdown section, artifact family coverage table per platform.
5. Regression tests: for each new parser family, verify expected JSON matches fixture output; verify field guarantee levels are accurate.
6. Documentation: `docs/linux-artifact-coverage.md`, `docs/mac-artifact-coverage.md`, `docs/pst-ost-mbox-support.md`.
7. Update `docs/parser-support-matrix.md` with all new parsers.
8. Update `docs/known-unsupported-formats.md` with explicit Linux/macOS gaps (ext4 raw disk, APFS, etc.).

#### Acceptance Criteria

- PST (Unicode 32/64), OST, and mbox parsing passes expected JSON regression on public-small and public-medium fixtures.
- Registry transaction log parsing extracts CreateKey/SetValue/DeleteKey/DeleteValue entries with timestamps; integrated into timeline precision.
- Chrome, Edge, and Firefox parsers updated to current schema and moved from Experimental to Supported with public-medium fixtures.
- Linux artifacts v1: systemd journal, wtmp, bash history, apt/dpkg, cron, sudo all with public-small fixtures and expected JSON.
- macOS artifacts v1: plist (5+ forensic plists), unified log, Spotlight, Quarantine all with public-small fixtures and expected JSON.
- All new parsers appear in `docs/parser-support-matrix.md` with platform, artifact family, field guarantee levels, and known limitations.
- Nested container chains (PST in E01, mbox in tar in E01) render correctly in the evidence graph.
- No regression in existing Windows artifact parsing or V2 regression tests.
- `/v3` dashboard shows multi-platform coverage statistics.
- Report export includes platform breakdown and artifact coverage per platform.

---

### Stage V3-3: Reproducible Investigation & Rule Pack System

#### Objective

Make investigations reproducible by introducing the Case Notebook with evidence citations, step recording/replay, and a declarative rule pack system. An investigator should be able to open a case from any point, see exactly what was done, replay the analysis steps, and understand why every lead was generated. Rule packs become the mechanism for sharing, validating, and governing correlation logic.

#### Stage boundaries

**In scope:**
- **Case Notebook**: timestamped, threaded notebook entries with structured fields (finding, hypothesis, action item, conclusion). Rich text entry (markdown). Evidence citations embedded in entries — a citation is a typed pointer to any graph node (File, Artifact, TimelineEvent, Lead, other NotebookEntry).
- **Step Recording**: automated capture of investigator actions (import, search, filter, correlate, tag, annotate, export) as `InvestigationStep` records with parameter snapshots and case-state hashes. Steps are recorded with timestamps and user identity.
- **Step Replay**: ability to replay a sequence of recorded steps (import replay is "re-run import with same sources"; query replay re-executes saved parameters). Replay produces a diff: what matches, what differs (non-deterministic results flagged).
- **Rule Pack Format**: TOML-based rule pack specification. Each pack has a manifest (name, version, author, description, scope, expected fixture hashes, min product version, caveats) and rule definitions (source node types, target node types, edge type, match conditions, confidence, caveats). Rule conditions are field-level predicates (equals, contains, regex, temporal proximity, path prefix).
- **Rule Pack Engine**: load, validate, and execute rule packs against the evidence graph. Rules produce CorrelatesWith edges with provenance back to the rule pack + rule id. Rule execution is incremental: loading a new pack runs its rules against the existing graph without full re-correlation.
- **Rule Pack Validation**: on load, validate rule pack schema, check fixture hashes, warn about conflicts/overlaps with existing packs, run against built-in fixtures and compare to expected output.
- **Org-Level Verification Config**: a case can declare required rule packs. The release scorecard gate checks that all required packs have been executed and their findings reviewed. An org can ship a `baseline.toml` pack that encodes mandatory checks.
- **Frontend**: NotebookPanel (listing, entry editor, citation picker, threading), RulePackManager (load/validate/list packs), StepHistory panel (categorized action log, replay controls).
- **Report integration**: notebook entries with embedded citations, step history as investigation timeline, rule pack coverage table, replay summary.

**Deferred:**
- Collaborative notebooks (multi-user editing, comments) — V3 is single-user
- Full step-replay of UI interactions (record mouse/keyboard macro) — only semantic steps (import, query, etc.) are recorded
- Rule pack marketplace or online sharing — rule packs are local files; sharing is manual
- Rule pack DSL with arbitrary logic — conditions are declarative field predicates only; no scripting
- Automatic evidence tampering detection via case-state hash comparison — recording only; tamper analysis is V4
- AI-assisted notebook entry generation — V4

#### Phase Tasks

**Phase 1: Case Notebook Backend (weeks 1-3)**
1. Define domain types: `NotebookEntry` (id, case_id, parent_entry_id, author, created_at, updated_at, entry_type: Observation/Hypothesis/Finding/ActionItem/Conclusion, title, body_markdown, tags, status: Draft/Reviewed/Final), `EvidenceCitation` (id, entry_id, target_node_type, target_node_id, display_label, snippet, cited_at).
2. Define DTOs: `NotebookEntryDto`, `EvidenceCitationDto`, `InvestigationStepDto`, `StepReplayDto`, `NotebookExportDto` in `crates/transport/src/dto/notebook.rs`.
3. Create SQLite tables in `crates/persistence-sqlite/`: `notebook_entries` (with recursive CTE support for threading), `evidence_citations`, `investigation_steps`.
4. Build `notebook_repo`: `create_entry`, `update_entry`, `get_entry`, `list_entries(case_id, filters)`, `get_thread(entry_id) -> Vec<NotebookEntry>`, `add_citation`, `list_citations_for_entry`, `delete_entry` (soft delete).
5. Build `investigation_step_repo`: `record_step`, `list_steps(case_id, step_kind_filter, time_range)`, `get_replay_sequence(from_step_id, to_step_id)`.
6. Build `notebook_service` in `crates/app-services/src/notebook_service.rs`: CRUD for entries, citation linking, step recording trigger, replay orchestration.
7. Tauri commands: `create_notebook_entry`, `update_notebook_entry`, `list_notebook_entries`, `get_notebook_thread`, `add_evidence_citation`, `remove_evidence_citation`, `record_investigation_step`, `list_investigation_steps`, `replay_investigation_steps`.
8. Register all notebook commands in `lib.rs`.

**Phase 2: Step Recording & Replay (weeks 3-5)**
1. Instrument existing Tauri commands (import, search, filter, correlate, tag, annotate, export) to record `InvestigationStep` on execution.
2. Step recording captures: command name, parameters (sanitized — no raw file paths outside case root, no credentials), timestamp, duration (on completion), case_state_hash (hash of case metadata + graph node/edge counts at step execution), success/failure, error code if failed.
3. Define `case_state_hash` computation: deterministic hash over (case id, data source list + hashes, file count, artifact count, timeline event count, graph node/edge counts). Fast to compute (O(1) from counters), changes detect import/analysis progress.
4. Step replay: for deterministic steps (search, filter, correlate with same parameters), re-execute and compare results. Flag differences.
5. For import replay: prompt investigator to confirm source availability, re-run import, compare resulting file/artifact/timeline counts.
6. Replay summary: a list of steps with "replayed and matched", "replayed with differences" (show diff), "could not replay" (source unavailable, non-deterministic).
7. Step history UI: categorized list, filterable by step kind and time range, click to see parameters and result summary.
8. Replay UI: select step range, click "Replay", see progress and result comparison.

**Phase 3: Rule Pack Format & Engine (weeks 5-8)**
1. Define rule pack TOML schema with formal spec in `docs/rule-pack-spec.md`.
2. Schema structure:
   ```toml
   [manifest]
   name = "standard-windows-user-activity"
   version = "1.0.0"
   author = "Org Name"
   description = "Standard Windows user activity correlation rules"
   scope = ["Windows"]
   min_product_version = "3.0.0"
   expected_fixtures = [
     { name = "medium-windows-case", sha256 = "abc123", expected_lead_count = 42 }
   ]
   
   [caveats]
   general = "These rules assume standard Windows 10/11 configurations."
   
   [[rules]]
   id = "prefetch-executable-to-file"
   name = "Prefetch Executable → File Entry"
   description = "Links Prefetch artifact executable paths to corresponding File entries."
   source_type = "Artifact"
   source_family = "Prefetch"
   target_type = "File"
   edge_type = "CorrelatesWith"
   
   [rules.condition]
   field = "source.executable_path"
   operator = "path_equals"
   target_field = "target.path"
   
   [[rules.condition]]
   field = "source.executable_path"
   operator = "filename_equals"
   target_field = "target.name"
   
   [rules.match_signals]
   confidence = "Strong"
   
   [rules.caveats]
   general = "Matches require exact path or name match. Case-insensitive on Windows."
   ```
3. Parser: `RulePackParser` in `crates/app-services/` that reads TOML, validates against schema, resolves `source_family`/`target_type` to graph node types, compiles field predicates.
4. Rule execution engine: for each rule, query graph for source nodes matching `source_type` + `source_family`, apply field predicates against candidate target nodes, create CorrelatesWith edges with `rule_provenance: { pack_id, rule_id, pack_version }`.
5. Incremental execution: track which (pack_id, rule_id) combinations have been executed for a case. Loading a new pack or updating a pack only runs new/changed rules.
6. Rule pack validation: on load, parse and validate TOML schema, check that all referenced `source_family` and `target_type` values are known to the product, warn if `expected_fixtures` hashes don't match installed fixtures. Score: valid, valid_with_warnings, invalid.
7. Rule pack storage: packs stored in a case-accessible directory (`case_root/rule_packs/` loaded packs, with provenance of load time and validator).
8. Extension: existing hardcoded V2 correlation rules are shipped as a built-in `v2-standard.toml` rule pack loaded by default for backward compatibility. Deprecation path: new cases use V3 rule packs; V2 cases can have the built-in pack auto-loaded.

**Phase 4: Rule Pack Frontend & Governance (weeks 8-10)**
1. Mirror rule pack DTOs in `frontend/src/types/models.ts`.
2. API module `frontend/src/lib/api/rule-packs.ts`: `loadRulePack`, `validateRulePack`, `listLoadedPacks`, `getRulePackCoverage`, `unloadRulePack`.
3. React Query hooks in `frontend/src/features/rule-packs/hooks.ts`.
4. `RulePackManager` component: list loaded packs with status (valid/warnings/errors), load new pack (file picker with `.toml` filter), validate feedback panel, rule count per pack, coverage summary.
5. `RulePackCoverage` chart: which rule families are covered by which packs, overlap/conflict indicators.
6. Org verification config: a `verification.toml` in the case directory specifies required rule packs and minimum review status. Case UI shows verification status: "3/5 required packs loaded and reviewed."
7. Release scorecard integration: new `rule-pack-coverage` gate in `V3GovernanceSnapshotDto`. Gate passes if all required packs are loaded, valid, and executed.
8. Report export: "Rule Pack Coverage" section listing loaded packs, rule counts, lead counts per pack.

**Phase 5: Frontend — Notebook & Integrated Investigation UI (weeks 10-12)**
1. Mirror notebook DTOs in `frontend/src/types/models.ts`.
2. API module `frontend/src/lib/api/notebook.ts`.
3. React Query hooks in `frontend/src/features/notebook/hooks.ts`.
4. `NotebookPanel` component: entry list with filters (type, tag, status, date), threaded view, inline markdown rendering, entry editor (textarea with markdown preview), citation picker.
5. Citation picker: when editing a notebook entry, investigator can search/select any graph node (File by path, Artifact by type+name, TimelineEvent by timestamp+description, Lead by summary, other NotebookEntry by title) and embed it as a citation. Citations render as clickable links that navigate to the cited item.
6. `StepHistory` component: categorized action log (Imports, Searches, Filters, Correlations, Tags, Exports), expandable parameter details, replay button.
7. Integration: clicking a citation in the notebook navigates to the relevant view (FileBrowser, Artifacts detail, Timeline, CorrelationWorkspace) and highlights the cited item.
8. Report integration: notebook entries with embedded citations in HTML/JSON/CSV export. Step history as an "Investigation Timeline" appendix.

**Phase 6: Documentation & Walkthrough (weeks 12-13)**
1. `docs/case-notebook-design.md` finalized.
2. `docs/rule-pack-spec.md` finalized with full TOML schema and example packs.
3. Investigator walkthrough: create a medium case, record a full investigation (import → search → filter → correlate → annotate findings → export report), replay it, verify reproducibility. Document as a tutorial.
4. Rule pack authoring guide: how to write, validate, and share rule packs.
5. Org verification guide: how to set up a `verification.toml`, required packs, and integrate with release governance.

#### Acceptance Criteria

- Notebook: create, edit, thread, cite evidence (File, Artifact, TimelineEvent, Lead, NotebookEntry), and export entries with citations.
- Step recording: all major Tauri commands (import, search, filter, correlate, tag, export) produce an `InvestigationStep` record automatically.
- Step replay: replay a sequence of search/filter/correlate/export steps and get a comparison report. At least one end-to-end walkthrough produces matching replay.
- Rule pack parsing: valid TOML packs load and validate; invalid packs produce clear error messages referencing the violating field.
- Rule pack execution: loading a pack runs its rules against the graph, producing CorrelatesWith edges with rule provenance. Incremental loading only runs new rules.
- Built-in `v2-standard.toml` pack reproduces all V2 correlation rules and passes on medium fixtures.
- At least 3 example rule packs ship with the product: `v2-standard.toml`, `windows-user-activity.toml`, `browser-analysis.toml`.
- `/v3` dashboard shows rule pack coverage and verification status.
- Release gate `rule-pack-coverage` enforces required-pack loading.
- No regression in existing functionality or V2 regression tests.
- All existing V2 correlation test assertions still pass (via built-in rule pack execution).

---

### Stage V3-4: Offline Batch Processing & V3 Release

#### Objective

Enable processing of very large cases (multi-TB, multi-source) via a recoverable, queueable, phased local batch subsystem. Complete the V3 release cycle with a full governance drill, scorecard gate verification, and release candidate deployment.

#### Stage boundaries

**In scope:**
- **Batch job model**: a `BatchJob` represents a queued or running multi-phase processing job. Phases: Mount → Catalog → ExtractArtifacts → Index → Correlate → (optional: Export). Phases have dependencies; later phases can start before earlier phases fully complete (streaming model within a phase, dependency gate at phase boundary).
- **Batch plan authoring**: an investigator defines a batch plan by selecting data sources, choosing which phases to run, setting resource limits (max memory, thread count), and setting phase options (artifact families to extract, index scope).
- **Checkpointing & recovery**: each phase writes a checkpoint on completion (or periodically for long phases). On interruption (crash, power loss, user cancel), the batch can resume from the last checkpoint. Phases that were partially complete are rolled back to checkpoint and restarted.
- **Progress persistence**: batch job state survives application restart. Progress is queryable at any time. Completed phases are never re-run unless the investigator explicitly resets them.
- **Resource governance**: batch job respects resource limits (memory ceiling, thread pool size). No single batch job can consume more than the configured limit. Multiple batch jobs are serial (simultaneous batches are out of scope for V3).
- **Partial results**: while a batch runs, completed phases are visible in the UI. An investigator can browse the file tree and search index while artifact extraction is still running.
- **Queue management**: list active/paused/completed/failed batch jobs. Pause/resume/cancel operations. View phase-level progress and logs.
- **Frontend**: BatchPlanBuilder (multi-step form for defining plans), BatchMonitor (dashboard showing active job, phase progress bars, logs, ETA), BatchHistory (list of past batch jobs with results).
- **Release governance**: final V3 release scorecard, full regression drill (fixture, security, performance, rule pack), `/v3` dashboard with all signals, release documentation.

**Deferred:**
- Simultaneous/concurrent batch jobs (serial only in V3)
- Distributed batch processing (cloud workers, network orchestration)
- Batch job scheduling (time-based triggers, cron) — manual start only
- Incremental case update (re-processing only changed sources) — V4
- Batch job via CLI (headless, no GUI) — desktop-only in V3

#### Phase Tasks

**Phase 1: Batch Subsystem Architecture (weeks 1-3)**
1. Define domain types: `BatchJob`, `BatchPlan`, `BatchPhase`, `PhaseKind` (Mount, Catalog, ExtractArtifacts, Index, Correlate, Export), `PhaseState` (Queued, Running, Paused, Completed, Failed), `BatchResourceLimits`.
2. Define DTOs: `BatchJobDto`, `BatchPlanDto`, `BatchPhaseDto`, `BatchResumeDto` in `crates/transport/src/dto/batch.rs`.
3. SQLite tables: `batch_jobs`, `batch_phases`, `batch_checkpoints`. Checkpoint table stores serialized phase state for resume.
4. Build `batch_repo` in `crates/persistence-sqlite/`: `create_job`, `get_job`, `list_jobs(case_id)`, `update_job_status`, `set_phase_state`, `write_checkpoint`, `read_checkpoint`.
5. Build `batch_service` in `crates/app-services/src/batch_service.rs`: `create_batch_plan`, `start_batch`, `pause_batch`, `resume_batch`, `cancel_batch`, `get_batch_status`, `get_phase_progress`, `get_batch_logs`.
6. Resource governor: `ResourceGovernor` struct that enforces memory ceiling and thread pool limits. Batch service checks resource availability before starting a phase; refuses to start if insufficient resources.
7. Checkpoint strategy: define what constitutes a checkpoint per phase kind (Catalog: committed file entries up to path X; ExtractArtifacts: completed artifact family Y; Index: indexed document count; Correlate: completed rule pack Z).

**Phase 2: Phase Implementation (weeks 3-6)**
1. Refactor existing `IngestPipeline` to be phase-aware: each ingest operation tracks its `PhaseKind` and reports progress in a format compatible with `BatchPhaseDto`.
2. **Mount phase**: open evidence sources, detect volumes/filesystems, validate integrity. Checkpoint: all sources mounted and validated.
3. **Catalog phase**: walk filesystem tree, create File nodes in graph, compute hashes. Checkpoint: directory path prefix completed. Supports pause/resume at directory boundary.
4. **ExtractArtifacts phase**: run artifact parsers against cataloged files. Checkpoint: artifact family completed. Supports filtering by artifact family (investigator selects which families to extract).
5. **Index phase**: build/increment search index (tantivy), build catalog index. Checkpoint: indexed document count.
6. **Correlate phase**: execute loaded rule packs against the graph. Checkpoint: rule pack id completed.
7. **Export phase** (optional): generate reports. Checkpoint: export format completed.
8. Progress reporting: each phase reports `(completed_units, total_units, unit_description, elapsed, estimated_remaining)` at regular intervals.
9. Batch logs: each phase writes structured log entries (info, warning, error) that are queryable from the frontend.

**Phase 3: Recovery & Robustness (weeks 6-8)**
1. Crash recovery: on application start, check for incomplete batch jobs. Verify checkpoint integrity (hash check). Present resume option to investigator.
2. Partial failure handling: a phase that fails does not abort the entire batch. Failed phase is marked Failed with error details. Investigator can retry the failed phase or skip it.
3. Cancel handling: cancel request triggers cooperative cancellation in the running phase. Phase writes a cancel checkpoint before stopping. No dirty state; graph is consistent at phase boundary.
4. Resource exhaustion handling: if a phase exceeds its memory limit, it fails gracefully (not OOM crash). Memory monitor checks RSS periodically; if approaching limit, phase checkpoints and pauses.
5. Disk space monitoring: before starting a phase, estimate required disk space (for evidence cache, index, temp files). Warn if free space is below threshold. During execution, monitor and pause if free space drops critically.
6. Batch job migration: support forward compatibility so batch jobs from V3-4.x can resume after a product update (minor version only). Store product version in batch job metadata.
7. Robustness tests: kill process mid-phase, verify resume; fill disk mid-phase, verify graceful pause; inject parse errors, verify partial artifact extraction.

**Phase 4: Frontend Batch UI (weeks 8-10)**
1. Mirror batch DTOs in `frontend/src/types/models.ts`.
2. API module `frontend/src/lib/api/batch.ts`: `createBatchPlan`, `startBatch`, `pauseBatch`, `resumeBatch`, `cancelBatch`, `getBatchJob`, `listBatchJobs`, `getBatchLogs`.
3. React Query hooks in `frontend/src/features/batch/hooks.ts`.
4. `BatchPlanBuilder`: multi-step form. Step 1: select data sources. Step 2: select phases. Step 3: configure resource limits (with sensible defaults based on host machine specs). Step 4: review & start.
5. `BatchMonitor`: dashboard card showing active job name, overall progress bar, phase list with individual progress bars + state indicators, elapsed time, ETA, log tail (last 20 lines). Pause/Resume/Cancel buttons with confirmation dialogs.
6. `BatchHistory`: list of past batch jobs with status, duration, result summary (files cataloged, artifacts extracted, leads found). Click to view detailed phase results and logs.
7. Partial results access: while a batch is running, the main UI shows "Batch in progress" banner with quick link to BatchMonitor. File browser, search, and timeline are available showing results from completed phases.
8. Tauri event integration: backend emits `batch:phase-changed` and `batch:progress` events; frontend subscribes via `EventBus` for live updates in `BatchMonitor`.
9. Zustand store `useBatchStore` for active batch state, used by `BatchMonitor` and the in-progress banner.

**Phase 5: V3 Release Governance (weeks 10-12)**
1. Finalize `V3GovernanceSnapshotDto` with all signals:
   - Graph statistics (node/edge counts, density, largest component)
   - Platform coverage (Windows/Linux/macOS artifact families, fixture counts)
   - Rule pack coverage (loaded packs, rule counts, lead counts, verification status)
   - Batch processing (active/completed job counts, average phase durations)
   - Error taxonomy (error counts by category, error rate per parser family)
   - Support matrix (parser families, coverage %, field guarantee levels)
   - Release gates (all V2 gates + new V3 gates: `graph-integrity`, `platform-coverage-minimum`, `rule-pack-required-loaded`, `batch-recovery-tested`)
2. V3 release scorecard: extend V2 scoring with V3 dimensions (graph integrity 15pts, platform coverage 15pts, notebook & replay 15pts, rule packs 15pts, batch processing 15pts, release governance 15pts, V2 carry-forward 10pts = 100pts).
3. RC drill: execute full regression suite on release candidate:
   - All V2 + V3 fixture regression (public-small, public-medium, private-real)
   - Security regression (export path safety, MCP permission model, media handle lifecycle, error desensitization)
   - Performance regression (benchmark baseline for medium/large across import, search, timeline, file-tree)
   - Rule pack regression (all shipped packs pass on corresponding fixtures)
   - Batch processing regression (create, run, pause, resume, crash-recover batch jobs)
   - Notebook & replay regression (create notebook with citations, record steps, replay full investigation)
4. Release scorecard gate: all hard gates pass; soft gates have documented risk acceptance.
5. Documentation: `docs/v3-release-scorecard.md`, `docs/v3-to-v4-migration.md`, updated `docs/parser-support-matrix.md`, `docs/error-taxonomy.md`.
6. Release blog / changelog for V3.0.0.
7. Archive V2 governance artifacts; mark V2 as "superseded by V3" in documentation.

**Phase 6: V3 Polish & Hardening (weeks 12-13)**
1. Performance profiling pass: identify and fix any regressions introduced by graph indirection or batch overhead.
2. UI polish: consistent loading states, error boundaries, empty states across all new views.
3. Accessibility audit: keyboard navigation for NotebookPanel, BatchMonitor, RulePackManager.
4. Error message audit: all new error paths produce user-actionable messages (no raw stack traces, no internal paths).
5. Memory leak check: long-running batch jobs must not leak memory; graph queries must release temporary allocations.
6. Final `/v3` dashboard QA: all signals update live, no stale data, graceful degradation when data is unavailable.
7. V3 tag and branch cut.

#### Acceptance Criteria

- Batch job creation, start, pause, resume, and cancel all work without data corruption or graph inconsistency.
- Crash recovery: kill the application mid-batch-phase, restart, resume from checkpoint, verify results match a clean-run batch.
- Resource governance: batch respects memory ceiling and thread pool limits; approaching limits triggers checkpoint + pause, not crash.
- Partial results: while a batch runs Catalog, the File Browser is available and shows files cataloged so far.
- Batch plan with at least 4 phases (Mount, Catalog, ExtractArtifacts, Correlate) executes end-to-end on a medium dataset.
- Batch UI (PlanBuilder, Monitor, History) passes frontend tests and manual QA.
- V3 release scorecard >= 85 points (B grade or higher) with all hard gates passing.
- Full RC drill completed: all regression suites pass, all gates green.
- `/v3` dashboard displays all governance signals live.
- All V2 regression tests still pass.
- No P0/P1 performance regressions relative to V2 baseline.
- Documentation complete: release scorecard, migration guide, updated support matrix.

---

## 4. Test Matrix

| Dimension | Scenario | Pass Criteria |
|-----------|----------|---------------|
| **Graph Integrity** | Fresh import of each public-medium fixture | Node/edge counts by type match expected values; regression automated |
| **Graph Traversal** | File → Artifact → TimelineEvent → Entity (depth 4) | All expected paths found; no missing edges; query < 200ms for depth-4 from single node (medium case) |
| **Graph Persistence** | Close and reopen case | Graph loads from SQLite with identical node/edge counts as at close time |
| **Graph Provenance** | Every CorrelatesWith edge | Edge carries rule id / parser version / extraction timestamp |
| **PST Parsing** | Unicode 32/64-bit PST, OST, mbox (public-small + public-medium) | Expected JSON matches; attachment extraction fidelity >= 95%; folder structure correct |
| **Registry TX Log** | Synthetic + real hive with transaction log | Operation sequence matches ground truth; timestamps correct |
| **Browser Refresh** | Chrome 130+, Edge 130+, Firefox 130+ (public-medium) | History, downloads, cookies, session restore all parse; expected JSON matches |
| **Linux Artifacts** | Systemd journal, wtmp, bash hist, apt/dpkg, cron, sudo (public-small each) | Expected JSON matches; timestamps correct; entity links created |
| **macOS Artifacts** | Plist (5+ types), unified log, Spotlight, Quarantine (public-small each) | Expected JSON matches; no binary plist decode errors on valid fixtures |
| **Container Nesting** | PST inside E01 inside raw; mbox inside tar inside E01 | Full chain parses correctly; graph contains container→contained edges |
| **Platform Discrimination** | Case with Windows + Linux + macOS artifacts | Graph nodes carry platform tag; `/v3` shows per-platform counts; cross-platform rules tagged |
| **Notebook CRUD** | Create, edit, thread, cite, export entries | Citations navigate to correct evidence; threading preserves order; markdown renders correctly |
| **Evidence Citation** | Cite File, Artifact, TimelineEvent, Lead, NotebookEntry | Citation target resolves to correct graph node; display label matches source item |
| **Step Recording** | All major commands (import, search, filter, correlate, tag, export) | Step recorded with params, timestamp, case_state_hash; no sensitive data in params |
| **Step Replay** | Replay search + filter + correlate sequence | Replay matches original results; differences flagged with explanation |
| **Rule Pack Schema** | Load valid packs; reject invalid packs | Valid packs parse and validate; invalid packs produce specific error messages |
| **Rule Pack Execution** | Load pack → run rules → verify CorrelatesWith edges | Edge counts match expected; provenance references pack + rule id; incremental load only runs new rules |
| **Rule Pack Governance** | Required packs loaded per verification.toml | `/v3` shows verification status; release gate passes when all required packs executed |
| **Built-in v2-standard.toml** | Run on medium Windows fixture | Produces same leads as V2 hardcoded rules; lead counts match V2 baseline |
| **Batch Create & Run** | 4-phase batch on medium dataset | All phases complete; graph populated; results match interactive import |
| **Batch Pause/Resume** | Pause mid-catalog, resume | No duplicate File nodes; catalog picks up at checkpoint; final counts match clean run |
| **Batch Crash Recovery** | Kill process mid-phase, restart | Batch detected as incomplete; resume offered; resume produces same results as clean run |
| **Batch Resource Limits** | Set memory ceiling to 500MB for medium case | Batch phases stay within limit; approaching limit triggers checkpoint + pause |
| **Batch Partial Results** | Batch running Catalog phase | File Browser available with in-progress catalog; search returns results for indexed files |
| **Batch UI** | PlanBuilder, Monitor, History | Plan builder validates inputs; monitor shows live progress; history lists past jobs correctly |
| **Performance — Import** | Medium case import with graph population (warm) | p95 <= V2 baseline +15%; no regression in large case |
| **Performance — Graph Query** | Depth-4 traversal from single node (medium case) | p95 <= 200ms |
| **Performance — Batch** | Medium case batch (Catalog + ExtractArtifacts) | Total wall time <= interactive import * 1.3 |
| **Security — Export** | Path traversal, overwrite, cross-case handle attempts | All rejected; audit log entries written |
| **Security — MCP** | Invalid SSE URL, embedded credential in URL, path-based stdio command | All rejected; audit log entries written |
| **Security — Error Desensitization** | Trigger parse/system/connection errors | Frontend receives error codes, not raw paths/credentials/environment |
| **Documentation Drift** | Support matrix, known limitations, error taxonomy | All updated to reflect V3 state; automated drift check passes |

---

## 5. Scoring Mechanism

Total: 100 points. Stage target scores and hard gates apply simultaneously. A hard gate failure sets the overall grade to D regardless of point total.

### 5.1 Score Composition

| Stage | Weight | Focus |
|-------|--------|-------|
| V3-1: Evidence Graph Foundation | 25 pts | Graph schema, population, query, migration of all views |
| V3-2: Container & Cross-Platform Coverage | 25 pts | PST/OST/mbox, Registry TX, browser refresh, Linux v1, macOS v1 |
| V3-3: Reproducible Investigation & Rule Packs | 25 pts | Notebook, citations, step replay, rule pack engine, governance |
| V3-4: Offline Batch Processing & Release | 25 pts | Batch subsystem, recovery, resource governance, RC drill, release |

### 5.2 Per-Stage Scoring Rules

- **100%**: All acceptance criteria met; no new unregistered P0/P1 risks; automated regression passing.
- **80%**: Core acceptance criteria met; P2 risks registered with mitigation plans; known limitations documented.
- **60%**: Main path usable but gaps in automation, fixture coverage, or edge-case handling.
- **0%**: Hard gate failure in that stage.

### 5.3 Hard Gates

The following are hard gates — failure of any gate sets overall grade to D regardless of point total:

- **Graph Integrity**: Any fixture import produces wrong node/edge counts (off by >1% from expected).
- **PST/OST/mbox Fixture Regression**: Any public fixture produces output that differs from expected JSON on guaranteed fields.
- **Evidence Citation Integrity**: A citation resolves to a different or non-existent graph node (broken reference).
- **Rule Pack Validation Bypass**: An invalid rule pack is accepted and executed without error.
- **Batch Data Corruption**: A batch resume or crash-recovery produces different graph state than a clean run.
- **Security Boundary Breach**: Export, MCP, or media handle boundary is circumvented (reproducible).
- **V2 Regression**: Any existing V2 regression test fails that is not a documented, approved deprecation.
- **Release Documentation Drift**: Support matrix or known limitations document is >1 release out of date at RC time.

### 5.4 Overall Grade Interpretation

- **A (90-100)**: Ready for V3 release. All hard gates pass; all stages >=80%.
- **B (80-89)**: Candidate release. All hard gates pass; at least 3 stages >=80%; remaining >=60%.
- **C (70-79)**: Internal test only. Hard gates pass but multiple stages <80%.
- **D (<70)**: Do not release. Hard gate failure or insufficient stage completion.

---

## 6. Agent Division & Collaboration

### 6.1 Fixed Division

- **Kepler** — Rust backend lead:
  - Graph schema, graph store, graph population pipeline, graph query engine
  - PST/OST/mbox parsing crate, Registry transaction log parser
  - Linux artifact parsers (crates/artifacts-linux/)
  - macOS artifact parsers (crates/artifacts-macos/)
  - Rule pack parser, validation engine, and execution engine
  - Batch subsystem (BatchService, ResourceGovernor, checkpointing, recovery)
  - Notebook service, step recording instrumentation, replay orchestration
  - All Tauri command implementations for V3 domains

- **Poincare** — Frontend lead:
  - Migration of all existing views to graph data source (FileBrowser, Timeline, Artifacts, CorrelationWorkspace)
  - `/v3` governance dashboard (graph stats, platform coverage, rule pack coverage, batch status)
  - NotebookPanel (entry list, editor, citation picker, threaded view)
  - RulePackManager (load/validate/list packs, coverage chart, verification status)
  - BatchPlanBuilder and BatchMonitor UI
  - StepHistory panel with replay controls
  - All new React Query hooks, API modules, mock data, and frontend tests
  - WebView2 media component compatibility with V3 case model
  - Accessibility and loading-state polish

- **Gauss** — Test & data asset lead:
  - Public-small and public-medium fixtures for PST/OST/mbox (synthetic + real)
  - Registry transaction log fixtures (synthetic + real)
  - Updated browser fixtures (Chrome 130+, Edge 130+, Firefox 130+)
  - Linux artifact fixtures per family (synthetic + VM snapshot)
  - macOS artifact fixtures per family (synthetic + VM snapshot)
  - Cross-platform multi-OS case fixture
  - Expected JSON for all new parsers
  - Graph integrity expected values (node/edge counts per fixture)
  - Batch processing test scenarios (crash recovery, resource exhaustion)
  - Notebook & replay test scenarios
  - Rule pack example packs and validation test cases
  - All regression test maintenance (V2 + V3)
  - Performance benchmark data for V3 graph and batch paths

- **Codex** — System integration & release lead:
  - IPC contract review (all new DTOs, events, command signatures)
  - Stage boundary enforcement (no cross-stage feature creep)
  - Mermaid/architecture documentation updates
  - Release scorecard assembly and gate verification
  - Risk register maintenance (V3 risk log)
  - Documentation drift checks (support matrix, known limitations, error taxonomy)
  - Final RC drill coordination
  - Dependency audit and advisory review
  - V3 release branch management
  - Migration path validation (V2 case → V3 case)

### 6.2 Collaboration Mechanism

- Stage order: `V3-1 → V3-2 → V3-3 → V3-4` as the main thread.
- Allowed parallelism:
  - V3-2 Phase 1 (PST/OST/mbox) can start once V3-1 Phase 2 (graph population) is stable — containers need the graph to write into.
  - V3-3 Phase 1 (Notebook backend) can start once V3-1 Phase 4 (frontend migration) is complete — notebook UI needs the graph-aware views.
  - V3-4 Phase 1 (Batch architecture) can start alongside V3-2 — designing the batch subsystem does not depend on cross-platform parsers being complete.
  - V3-2 Phase 4 (Linux) and Phase 5 (macOS) are independent and can run in parallel.
- Each Phase defaults to 2 weeks; each Stage ends with a 1-week integration sprint.
- Weekly fixed outputs from all agents:
  1. Change summary (what was done, what's in progress)
  2. Risk increments (new risks, escalated risks, resolved risks)
  3. Regression results (tests added, tests broken, tests fixed)
  4. Documentation sync status (which docs were touched, which need updates)

---

## 7. Assumptions & Defaults

- V3 builds on a completed V2. V2 closeout items (nightly regression, memory CI enforcement, full audit coverage, RC drill) are done before V3-1 Phase 1 begins.
- V3 maintains all architectural constraints from V1 and V2: Windows-primary, desktop-first, single-user, no HTTP server, Tauri-only IPC, `crates/transport` as sole contract source, evidence read-only, SQL behind service layer.
- PST/OST/mbox parsing aims for message + attachment + folder + calendar + contact extraction. Full MAPI property fidelity and encrypted message support are deferred to V4.
- Linux and macOS artifact parsing in V3 is file-tree-based: artifacts are extracted from imported file trees, not from raw disk images. Linux filesystem (ext4/XFS/Btrfs) and macOS filesystem (APFS/HFS+) parsing are V4.
- The V3 evidence graph is stored in the same SQLite database as the case (WAL mode, same connection pool). A dedicated graph database (e.g., Neo4j embedded) is not introduced — this is a desktop app, not a server.
- Rule packs use TOML as the format. YAML is considered but TOML is preferred for consistency with the Rust ecosystem (Cargo.toml) and better error locality. JSON is rejected because it lacks comments — rule packs must be human-authored and documented.
- Batch processing is single-job serial execution. Simultaneous batch jobs are out of scope. CLI/headless operation is out of scope. Cloud/distributed orchestration is out of scope.
- Notebook entries are local to a case. There is no cloud sync, sharing, or multi-user collaboration.
- Step recording captures semantic actions (command + parameters), not UI-level macros (mouse clicks, keystrokes). Replay re-executes commands, not UI interactions.
- Case-state hash uses graph counters for fast computation. It does not detect bit-level evidence tampering. Tamper detection is a V4 feature.
- The existing V2 correlation engine is refactored into a built-in `v2-standard.toml` rule pack for backward compatibility. New correlation logic is written as rule packs, not hardcoded Rust.
- In V3, Browser artifacts graduate from "Experimental" to "Supported" only after public-medium fixtures and field guarantee levels are established. Email parsing (EML/EMLX) similarly graduates.
- All new parsers (PST/OST/mbox, Linux, macOS) enter the V2-1 trust framework: public fixture, expected JSON, field guarantee levels, known limitations.
- The `public-small` and `public-medium` fixture designations follow V2-1 definitions. `private-real-regression` fixtures remain in a private repository; their SHA-256 hashes and expected output counts are committed to the public repo.
- Documents, fixture manifests, expected JSON, error taxonomy entries, benchmark records, and rule packs are all UTF-8.

---

## 8. V4 Directions (Preliminary)

These are speculative and should be refined during V3 execution. They are not commitments.

### 8.1 Advanced Entity Resolution
- Cross-source entity deduplication: merge Person entities derived from email addresses, usernames, and display names across artifacts
- Entity relationship inference: communication patterns (who emailed whom), file ownership, login sessions
- Entity timeline: all events involving a specific entity across all evidence sources
- Graph-based anomaly detection: unusual entity relationships, outlier access patterns

### 8.2 Multi-OS Disk Image Support
- Linux filesystem parsing from raw disk images: ext4, XFS, Btrfs
- macOS filesystem parsing from raw disk images: APFS, HFS+
- Cross-platform evidence acquisition: single case with Windows + Linux + macOS disk images
- File carving from Linux/macOS filesystems (deleted file recovery)

### 8.3 Advanced Mobile & Cloud Artifacts
- iOS backup/image parsing: SQLite databases (Contacts, Messages, Photos, Safari, etc.)
- Android backup/image parsing: SMS/MMS, contacts, app data, Chrome history
- Cloud artifact acquisition: AWS CloudTrail, Azure Audit logs, GCP Audit logs, Google Workspace logs, Microsoft 365 Unified Audit Log
- Cloud-to-local correlation: cloud log entries linked to local file/artifact evidence

### 8.4 AI-Assisted Investigation
- Notebook entry summarization: condense long entries into structured findings
- Lead narrative generation: explain why a set of leads is interesting in plain language
- Evidence gap analysis: identify what's missing based on investigative templates
- Search query assistance: natural language to structured search filters

### 8.5 Investigation Exchange Format
- Standardized case export/import format (beyond current report formats)
- Interchange with other forensic tools (Plaso timeline, Autopsy module format)
- Evidence package for external review (redacted, read-only case snapshot)
- Chain-of-custody integration: digital signatures on case artifacts and exports

### 8.6 Real-Time Evidence Acquisition
- Live-response agent: lightweight collection from a running Windows/Linux/macOS system
- Memory image acquisition and integration (alongside disk images in the same case)
- Network capture integration (PCAP ingestion, flow record parsing)
- Streaming evidence: process evidence as it arrives without waiting for full acquisition

### 8.7 Graph Query Language (GQL)
- DSL for querying the evidence graph with path patterns, filters, aggregations
- Autocomplete and syntax highlighting in the UI
- Saved queries as part of investigation templates
- Query explain/plan visualization for performance debugging

---

*V3 Plan version 1.0. Last updated: 2026-06-13.*
