# Forensics Workbench V4 Execution Plan

## 0. V4 Baseline (as of 2026-06-17)

V4 builds on a V3 that is ~92% complete (Grade A-, 1,345 Rust + 228 frontend tests, 8 performance bottlenecks fixed, 2 real E01 samples, 25 crates, 84 Tauri commands).

### Architectural constraints carried forward

- Windows-primary, desktop-first, single-user
- No HTTP server; Tauri commands/events only
- `crates/transport` is the sole IPC contract source
- Evidence is read-only
- SQL stays in `persistence-sqlite` or lower
- All new parsers enter V2-1 trust framework (fixture + expected JSON + guarantee levels)
- UTF-8 for all docs/fixtures/benchmarks

---

## 1. V4 Goals

V4 transforms Forensics Workbench from a "multi-platform forensic analysis tool" into a **professional investigation platform with advanced entity intelligence, cross-case correlation, and AI-assisted workflows**.

### Five pillars

1. **Advanced Entity Resolution & Cross-Case Intelligence** — Deduplicate entities across cases, infer relationships (communication patterns, file ownership, login sessions), build entity timelines, detect anomalous entity relationships through graph analysis.

2. **Multi-OS Raw Disk Image Support** — Extend beyond E01/NTFS to raw disk parsing for Linux filesystems (ext4, XFS, Btrfs) and macOS filesystems (APFS, HFS+). Enable single-case multi-OS disk image analysis.

3. **AI-Assisted Investigation** — Notebook entry summarization, lead narrative generation, evidence gap analysis, natural-language-to-structured-search query assistance. All AI features are local/offline (no cloud dependency).

4. **Investigation Exchange & Chain-of-Custody** — Standardized case export/import for inter-tool exchange (STIX 2.1, CASE/UCO), digital signatures on case artifacts and exports, chain-of-custody log with cryptographic verification.

5. **Real-Time & Streaming Evidence Acquisition** — Live-response agent for running systems, memory image integration, PCAP ingestion, streaming evidence processing (start analysis while acquisition is in progress).

---

## 2. Key Interfaces & DTOs

### 2.1 Entity Resolution DTOs

| DTO | Purpose |
|-----|---------|
| `ResolvedEntityDto` | Merged entity with confidence, source entities, canonical attributes |
| `EntityRelationshipDto` | Typed relationship: CommunicatesWith, Owns, LoggedInto, Executed, Downloaded |
| `EntityTimelineDto` | All events involving a specific entity across all evidence sources |
| `CrossCaseEntityMatchDto` | Entity matches across cases with confidence scores |
| `AnomalyDetectionResultDto` | Flagged unusual entity relationships or access patterns |

### 2.2 Raw Disk Image DTOs (extend existing)

| DTO | Purpose |
|-----|---------|
| `LinuxVolumeDto` | ext4/XFS/Btrfs volume metadata and mount info |
| `MacVolumeDto` | APFS/HFS+ volume metadata |
| `MultiOSProbeDto` | Combined probe result: Windows + Linux + macOS partitions |
| `RawDiskImageDto` | Enhanced from existing RawImageReader with multi-OS awareness |

### 2.3 AI Assistance DTOs

| DTO | Purpose |
|-----|---------|
| `NotebookSummarizationRequestDto` | Input: notebook entry IDs, output style |
| `LeadNarrativeDto` | Generated plain-language explanation of a lead cluster |
| `EvidenceGapAnalysisDto` | Identified missing evidence types based on investigation template |
| `NLQueryRequestDto` | Natural language → structured search filters |

### 2.4 Exchange & Chain-of-Custody DTOs

| DTO | Purpose |
|-----|---------|
| `CaseExportBundleDto` | Complete exportable case snapshot (redacted, signed) |
| `STIX21BundleDto` | STIX 2.1 indicators, observed-data, relationships |
| `ChainOfCustodyEntryDto` | Timestamped, signed custody event |
| `DigitalSignatureDto` | Ed25519 signature over case artifact hashes |

### 2.5 Streaming Acquisition DTOs

| DTO | Purpose |
|-----|---------|
| `LiveAgentConfigDto` | Collection profile for live-response agent |
| `StreamingEvidenceChunkDto` | Partial evidence data with progress metadata |
| `MemoryImageIntegrationDto` | Memory dump linked to disk image in same case |
| `PCAPSessionDto` | Network capture session metadata and packet summary |

---

## 3. Stage Design

### Stage V4-1: Advanced Entity Resolution（25 分，13 周）

#### Objective
Build cross-source entity deduplication and relationship inference on top of the V3 Evidence Graph.

#### Stage boundaries
**In scope**: Entity merging (Person: email+username+display_name), Device (hostname+MAC+volume_serial), entity relationship inference (communication, ownership, login), entity timeline, cross-case entity matching, graph-based anomaly detection (unusual relationships, outlier access).

**Deferred**: Full ML-based entity resolution, real-time entity streaming.

#### Phase Tasks

**Phase 1: Entity Canonicalization and Merge Engine (weeks 1-3)**

Domain & DTO:
- Create `crates/domain/src/entity.rs` — Entity, EntityType (Person/Device/Organization/Location), EntityId, CanonicalRepresentation, MergeRule
- Create `crates/transport/src/dto/entity.rs` — ResolvedEntityDto, EntityCandidateDto, MergeRequestDto, MergeResultDto, EntityAttributeDto
- Update `crates/transport/src/dto/mod.rs` — re-export entity DTO module
- Update `frontend/src/types/models.ts` — ResolvedEntity, EntityCandidate, MergeRequest, MergeResult, EntityAttribute interfaces

Persistence:
- Create `crates/persistence-sqlite/migrations/0028_entity_canonical.sql` — entity_canonical (id, entity_type, created_at, updated_at)
- Create `crates/persistence-sqlite/migrations/0029_entity_attributes.sql` — entity_attributes (entity_id, attr_name, attr_value, source, confidence)
- Create `crates/persistence-sqlite/migrations/0030_entity_sources.sql` — entity_sources (entity_id, source_entity_id, source_type, source_case_id)
- Create `crates/persistence-sqlite/migrations/0031_entity_merge_rules.sql` — entity_merge_rules (id, entity_type, match_attrs, threshold, is_active)
- Create `crates/persistence-sqlite/src/entity_repo.rs` — EntityRepo: create_entity, upsert_attribute, link_source, upsert_merge_rule, find_candidates_by_type, apply_merge, get_resolved_entity, list_merge_rules
- Add `mod entity_repo;` to `crates/persistence-sqlite/src/lib.rs`

Engine:
- Create `crates/app-services/src/entity_resolution/mod.rs` — EntityResolutionService, EntityResolutionConfig
- Create `crates/app-services/src/entity_resolution/merge.rs` — attribute merge: email exact match (confidence 1.0), username fuzzy+domain (0.85-0.95), display_name Jaro-Winkler (0.75-0.90 with corroboration); conflict resolution by source timestamp priority
- Create `crates/app-services/src/entity_resolution/canonicalize.rs` — canonical form: Person→primary email, Device→hostname+MAC, Organization→domain, Location→geo+network
- Create `crates/app-services/src/entity_resolution/scoring.rs` — attribute weight tables, confidence threshold per entity type
- Add `pub mod entity_resolution;` to `crates/app-services/src/lib.rs`

Tauri Commands:
- Create `apps/desktop/src-tauri/src/commands/entity_commands.rs` — merge_entities, get_resolved_entity, get_entity_candidates, list_merge_rules, accept_merge, reject_merge
- Register in `apps/desktop/src-tauri/src/lib.rs` invoke_handler

Frontend:
- Create `frontend/src/lib/api/entity.ts` — mergeEntities, getResolvedEntity, getEntityCandidates API
- Create `frontend/src/features/entity/hooks.ts` — useEntityMerge, useResolvedEntity, useEntityCandidates hooks
- Add mock data in `frontend/src/lib/api/mock-data.ts`

Tests:
- `crates/app-services/tests/entity_merge_tests.rs` — 8 tests: person merge (email, username+domain, display_name fuzzy), device merge (hostname+MAC, volume_serial), no-merge false positives, threshold boundary
- `crates/persistence-sqlite/tests/entity_repo_tests.rs` — 7 tests: CRUD, attribute upsert, source link, atomic merge, merge rules

**Phase 2: Entity Relationship Inference (weeks 3-6)**

Engine:
- Create `crates/app-services/src/entity_resolution/relationships.rs` — 5 relationship types: CommunicatesWith (email sender→recipient), Owns (NTFS SID→Person), LoggedInto (EVTX 4624→Device), Executed (Prefetch+Amcache→File), Downloaded (browser history→File); bidirectional edge creation; confidence scoring per relationship
- Create `crates/app-services/src/entity_resolution/graph_walk.rs` — V3 Evidence Graph traversal, batch edge insertion
- Create `crates/app-services/src/entity_resolution/relationship_batch.rs` — FK-safe edge insertion, duplicate detection, incremental update

DTO:
- Add to `crates/transport/src/dto/entity.rs`: EntityRelationshipDto, EntityRelationshipType enum, EntityRelationshipQueryDto, EntityGraphSnapshotDto
- Update `frontend/src/types/models.ts`

Persistence:
- Create `crates/persistence-sqlite/migrations/0032_entity_relationships.sql` — entity_relationships (id, source_entity_id, target_entity_id, relationship_type, confidence, evidence_refs JSON, created_at)
- Add to `crates/persistence-sqlite/src/entity_repo.rs`: insert_relationship, get_relationships_for_entity, get_relationship_graph

Commands:
- Add to `apps/desktop/src-tauri/src/commands/entity_commands.rs`: get_entity_relationships, get_entity_graph, infer_relationships

Frontend:
- Create `frontend/src/components/entity/EntityRelationshipGraph.tsx` — D3/visx force graph, colored edge types, hover tooltips, click-to-navigate

Tests:
- `crates/app-services/tests/entity_relationship_tests.rs` — 8 tests per relationship type + bidirectional + confidence + batch idempotency
- `crates/app-services/tests/entity_graph_walk_tests.rs` — full multi-source walk, performance under 100k nodes

**Phase 3: Cross-Case Entity Matching (weeks 6-8)**

Engine:
- Create `crates/app-services/src/entity_resolution/cross_case.rs` — CrossCaseEntityService: load from N cases, attribute comparison, configurable threshold (default 0.7), ranking, deduplication
- Create `crates/app-services/src/entity_resolution/cross_case_index.rs` — indexed matching (lowercase email/hostname), bloom-filter pre-filter, target < 5s for 3x10k
- Create `crates/app-services/src/entity_resolution/cross_case_store.rs` — cross_case_matches persistence, match status (pending/confirmed/rejected)

DTO:
- Add to `crates/transport/src/dto/entity.rs`: CrossCaseEntityMatchDto, CrossCaseMatchRequestDto, CrossCaseMatchResultDto
- Update `frontend/src/types/models.ts`

Persistence:
- Create `crates/persistence-sqlite/migrations/0033_cross_case_matches.sql`
- Add to entity_repo.rs: upsert_cross_case_match, list_matches, update_match_status

Commands:
- Add to entity_commands.rs: match_cross_case_entities, get_cross_case_matches, update_cross_case_match_status

Frontend:
- Create `frontend/src/app/pages/CrossCaseMatches.tsx` — match table, threshold slider, attribute comparison side-by-side, confirm/reject
- Add route `/cross-case-matches` in `frontend/src/app/routes.tsx`

Tests:
- `crates/app-services/tests/cross_case_match_tests.rs` — 6 tests: 2-case email match, 3-case device match, no false match, threshold config, performance, deduplication

**Phase 4: Entity Timeline and Anomaly Detection (weeks 8-10)**

Engine:
- Create `crates/app-services/src/entity_resolution/timeline.rs` — entity-centric timeline: merge events across relationships + V3 timeline, sort chronologically, annotate with source type and confidence
- Create `crates/app-services/src/entity_resolution/anomaly.rs` — 4 anomaly types: (1) OffHoursAccess (login outside mu±2sigma → severity >= 0.7 if >3sigma), (2) UnusualRelationship (new edge → severity = 1.0 - familiarity), (3) VolumeSpike (>3x rolling avg in 1h window), (4) CommunityOutlier (Louvain modularity → severity = 1.0 - modularity)
- Create `crates/app-services/src/entity_resolution/anomaly_scoring.rs` — severity normalization 0.0-1.0, type weighting

DTO:
- Add to `crates/transport/src/dto/entity.rs`: EntityTimelineDto, EntityTimelineEventDto, AnomalyDetectionResultDto, AnomalyDto, AnomalyType enum
- Update `frontend/src/types/models.ts`

Commands:
- Add to entity_commands.rs: get_entity_timeline, detect_entity_anomalies, get_anomaly_detail

Frontend:
- Create `frontend/src/components/entity/EntityTimeline.tsx` — color-coded vertical timeline, date range + event type filters
- Create `frontend/src/components/entity/AnomalyPanel.tsx` — severity badges (red >=0.8, yellow >=0.5, blue <0.5), type tabs, investigate links

Tests:
- `crates/app-services/tests/entity_timeline_tests.rs` — 5 tests: multi-source ordering, type filter, date range, max limit, empty entity
- `crates/app-services/tests/entity_anomaly_tests.rs` — 7 tests across all 4 anomaly types with positive and false-positive checks

**Phase 5: Frontend — Entity Explorer (weeks 10-12)**

- Create `frontend/src/app/pages/EntityExplorer.tsx` — type filter sidebar, attribute search, card grid, sort by name/type/relationships/last_seen
- Create `frontend/src/components/entity/EntityDetailCard.tsx` — canonical header, source table, relationship tabs, embedded timeline + anomaly panel
- Create `frontend/src/components/entity/MergeReviewPanel.tsx` — side-by-side diff, confidence gauge, accept/reject, undo snackbar (30s)
- Add routes: `/entities`, `/entities/:id`, `/entities/merge-review`
- Add mock data for all entity endpoints

**Phase 6: Documentation and Governance (weeks 12-13)**

- Create `docs/v4-entity-resolution.md` — architecture, merge strategy table, confidence model, relationship rules, anomaly algorithms
- Create `docs/v4-entity-resolution-test-plan.md` — coverage map, fixture requirements, acceptance procedures
- Update `CLAUDE.md` and `AGENTS.md` with entity resolution module descriptions
- Create `scripts/check-entity-resolution-boundary.ps1` — verify no SQL in commands, service calls through app-services only
- Run full V3 regression: `cargo test --workspace` and `pnpm --dir frontend test`

#### Test Matrix

| # | Test Name | Scenario | Expected Result | Phase |
|---|-----------|----------|-----------------|-------|
| ER-01 | `test_merge_person_by_email` | Two Person entities with identical email | confidence > 0.95, merged entity created | P1 |
| ER-02 | `test_merge_person_by_username_domain` | Matching username+domain across sources | confidence > 0.85 | P1 |
| ER-03 | `test_merge_person_by_display_name` | Jaro-Winkler > 0.85 + corroborating attr | confidence > 0.75 | P1 |
| ER-04 | `test_merge_device_by_hostname_mac` | Two Device entities: matching hostname+MAC | confidence > 0.95 | P1 |
| ER-05 | `test_merge_device_by_volume_serial` | Two Device entities: matching volume_serial | confidence > 0.85 | P1 |
| ER-06 | `test_no_merge_different_persons` | Distinct emails, similar display names | merge rejected, confidence < 0.5 | P1 |
| ER-07 | `test_no_merge_different_devices` | Same hostname, different MAC+volume_serial | merge rejected | P1 |
| ER-08 | `test_merge_threshold_boundary` | Threshold 0.9, candidate at 0.85 | not auto-merged, flagged for review | P1 |
| ER-09 | `test_communicates_with_from_email` | PST fixture: sender A→recipient B (3 emails) | CommunicatesWith A→B, confidence >= 0.9 | P2 |
| ER-10 | `test_owns_from_ntfs_ownership` | NTFS file with SID→Person mapping | Owns edge: Person→File, evidence_refs populated | P2 |
| ER-11 | `test_logged_into_from_evtx_4624` | EVTX 4624: User X, Workstation Y | LoggedInto: Person X→Device Y | P2 |
| ER-12 | `test_executed_from_prefetch` | Prefetch artifact: executable run by user | Executed: Person→File | P2 |
| ER-13 | `test_downloaded_from_browser_history` | Browser download history: known user profile | Downloaded: Person→File | P2 |
| ER-14 | `test_relationship_bidirectional` | CommunicatesWith A→B; query B's edges | A→B and B→A both present | P2 |
| ER-15 | `test_cross_case_match_email_2_cases` | Same email in Case 1 and Case 2 | match confidence > 0.9, DTO returned | P3 |
| ER-16 | `test_cross_case_match_device_3_cases` | Same MAC+hostname in 3 cases | all 3 pairwise matches found | P3 |
| ER-17 | `test_cross_case_no_false_match` | 3 people with similar-but-distinct attributes | no matches above 0.7 threshold | P3 |
| ER-18 | `test_cross_case_threshold_config` | Thresholds 0.6 / 0.8 / 0.95 | N1 >= N2 >= N3; N3 near zero | P3 |
| ER-19 | `test_cross_case_performance` | 3 cases x 10,000 entities each | computation < 5 seconds | P3 |
| ER-20 | `test_entity_timeline_multi_source` | EVTX + PST + LNK + Prefetch events | chronologically ordered across 4 sources | P4 |
| ER-21 | `test_entity_timeline_filter_type` | Filter to LoggedInto only | only LoggedInto returned | P4 |
| ER-22 | `test_entity_timeline_date_range` | Filter to [2024-01-01, 2024-01-07] | only events within range | P4 |
| ER-23 | `test_anomaly_off_hours_login` | Login at 03:00; mu=10:00, sigma=3h | anomaly flagged, severity >= 0.7 | P4 |
| ER-24 | `test_anomaly_off_hours_no_fp` | Login at 22:00; mu=20:00, sigma=2h | severity < 0.5 | P4 |
| ER-25 | `test_anomaly_unusual_relationship` | Person→Device edge never seen before | anomaly flagged, severity >= 0.6 | P4 |
| ER-26 | `test_anomaly_volume_spike` | File accesses 5x rolling 24h average | anomaly flagged, severity >= 0.8 | P4 |
| ER-27 | `test_anomaly_community_outlier` | Modularity < 0.3 to Louvain community | anomaly flagged, severity >= 0.5 | P4 |
| ER-28 | `test_entity_explorer_page_load` | /entities with 5,000 entities | load < 2s, type filter counts correct | P5 |
| ER-29 | `test_entity_detail_card_full` | Click resolved Person entity | all sections (attrs, relationships, timeline, anomalies) render | P5 |
| ER-30 | `test_merge_review_accept` | Accept proposed merge (confidence 0.85) | merge applied, entity list refreshes | P5 |
| ER-31 | `test_merge_review_reject` | Reject proposed merge | merge dismissed, entities remain separate | P5 |
| ER-32 | `test_merge_review_undo` | Accept then undo within 30s | entities unmerged, state restored | P5 |
| ER-33 | `test_v3_regression_after_entity` | Full V3 suite after entity tables added | 1,345 Rust + 228 frontend: 100% pass | P6 |
| ER-34 | `test_entity_resolution_guard` | check-entity-resolution-boundary.ps1 | no SQL in commands, service calls through app-services | P6 |

#### Acceptance Criteria

- **Person merge precision**: zero false merges on fixture with 100 known-distinct persons (precision = 1.0). Email exact: confidence >= 0.95. Username+domain fuzzy: >= 0.85. Display_name Jaro-Winkler >= 0.85: only merges with corroboration.
- **Device merge precision**: hostname+MAC composite: >= 0.95. Volume_serial: >= 0.85. Conflicting MAC on same hostname blocks merge.
- **Relationship inference**: 5 types inferred and persisted; recall >= 90% on synthetic ground-truth fixture.
- **Cross-case matching**: threshold configurable (default 0.7); latency < 5s for 3 cases x 10,000 entities; precision >= 95% on controlled fixture.
- **Entity timeline**: >= 3 evidence source types for well-connected entity; event type + date range filters; chronological order.
- **Anomaly detection**: precision >= 80% (16/20 detected) on labeled fixture; 4 anomaly types; severity calibrated 0.0-1.0.
- **Frontend UX**: list load < 2s for 5,000 entities; detail render < 500ms; search results < 300ms.
- **Merge review**: accept/reject in < 3 clicks; undo within 30s; attribute diff with source attribution visible.
- **V3 regression**: all 1,345 Rust + 228 frontend tests pass.
- **Guard script**: `check-entity-resolution-boundary.ps1` passes.
- **Docs**: `docs/v4-entity-resolution.md` complete with architecture, merge tables, confidence model, anomaly pseudocode.

#### Expected Results

**After Phase 1**: `cargo test -p app-services entity_merge` shows merge engine producing canonical entities with confidence scores. SQLite has `entity_canonical`, `entity_attributes`, `entity_sources`, `entity_merge_rules` tables. The `merge_entities` Tauri command returns `MergeResultDto` with merged entity ID and confidence breakdown.

**After Phase 2**: Importing E01 with email+EVTX+NTFS+Prefetch+browser artifacts auto-infers CommunicatesWith/Owns/LoggedInto/Executed/Downloaded edges. `get_entity_relationships` returns typed relationships with evidence citations. EntityRelationshipGraph component renders force-directed graph with color-coded edges.

**After Phase 3**: Opening 2+ cases shows cross-case matches at `/cross-case-matches` page. Threshold slider (0.5-1.0) updates match count in real time. Confirmed matches persist across sessions.

**After Phase 4**: Entity timeline displays chronologically ordered events color-coded by type (login=blue, communication=green, file=orange, execution=red). Anomalies appear as warning icons with severity badges. AnomalyPanel lists all detections with investigate links.

**After Phase 5**: Entity Explorer (`/entities`) in main nav. Browse by type (Person 45, Device 12...), search by attribute, click for full detail (graph, timeline, anomalies). Merge review panel shows side-by-side attribute diff with confidence gauges.

**After Phase 6**: Docs complete. All guard scripts pass. Full V3 regression green (1,345 + 228 tests).

---

### Stage V4-2: Multi-OS Raw Disk Image Support（25 分，14 周）

#### Objective
Extend evidence ingestion from E01/NTFS to raw disk images containing Linux and macOS filesystems.

#### Stage boundaries
**In scope**: ext4 (full: inode tables, extent trees, directory hashing), XFS (v5: B+tree AGs, inode B+tree), Btrfs (basic: chunk tree, extent tree, subvolume listing), APFS (basic: container superblock, volume listing, checkpoint mapping), HFS+ (full: catalog B-tree, attributes, hard links), deleted file recovery (ext4 journal replay, APFS checkpoint rewind), multi-OS probe (detect partition types across OS boundaries in a single GPT disk).

**Deferred**: Btrfs RAID/compression/subvolume snapshots, APFS encryption/snapshot/clone fidelity, ZFS support.

#### Phase Tasks

**Phase 1: ext4 Reader Crate (weeks 1-3)**

Crate scaffolding:
- Create `crates/fs-ext4/Cargo.toml` — workspace member, depends on evidence-core
- Create `crates/fs-ext4/src/lib.rs` — Ext4Reader, Ext4Config, Ext4Error
- Create `crates/fs-ext4/src/superblock.rs` — superblock (offset 1024): magic 0xEF53, block_size, inode_size, feature flags (extent, dir_index, journal, flex_bg, 64bit)
- Create `crates/fs-ext4/src/group_descriptor.rs` — block group descriptor, block/inode bitmaps
- Create `crates/fs-ext4/src/inode.rs` — inode table, extent tree walking, indirect block chains, file size/metadata/permissions/timestamps
- Create `crates/fs-ext4/src/extent.rs` — extent tree traversal: extent header, extent index, extent leaf nodes
- Create `crates/fs-ext4/src/directory.rs` — directory entries: linear (ext4_dir_entry_2) and HTree indexed (dx_root, dx_entry hash lookup)
- Create `crates/fs-ext4/src/symlink.rs` — fast symlink (< 60 bytes in inode) and slow symlink (data block resolution)
- Create `crates/fs-ext4/src/evidence_adapter.rs` — impl EvidenceReader trait: read_file, read_dir, stat, list_root
- Add `fs-ext4 = { workspace = true }` to root `Cargo.toml` members
- Register Ext4Reader in `crates/evidence-core/src/reader_registry.rs`
- Add `FsType::Ext4` variant to `crates/evidence-core/src/fs_type.rs`

Fixtures:
- Create `testdata/fixtures/public-small/ext4-extent-file.img` — superblock, 1 block group, extent-mapped file
- Create `testdata/fixtures/public-small/ext4-htree-dir.img` — HTree-indexed directory (100+ entries)
- Create `testdata/fixtures/public-small/ext4-symlink.img` — fast and slow symlinks
- Create `testdata/fixtures/public-small/ext4-indirect-block.img` — file using indirect block chain
- Create matching expected JSON files in `testdata/fixtures/expected/`
- Create `scripts/generate-ext4-fixtures.ps1`

Tests:
- `crates/fs-ext4/tests/superblock_tests.rs` — magic, block_size, feature flags parsing
- `crates/fs-ext4/tests/inode_tests.rs` — file/dir/symlink inode, extent parsing
- `crates/fs-ext4/tests/directory_tests.rs` — linear directory + HTree lookup by filename
- `crates/fs-ext4/tests/fixture_tests.rs` — all public-small fixtures → expected JSON on guaranteed fields
- `crates/fs-ext4/tests/evidence_adapter_tests.rs` — EvidenceReader trait conformance

**Phase 2: XFS Reader Crate (weeks 3-5)**

Crate scaffolding:
- Create `crates/fs-xfs/Cargo.toml`
- Create `crates/fs-xfs/src/lib.rs` — XfsReader, XfsConfig, XfsError
- Create `crates/fs-xfs/src/superblock.rs` — XFS v5 superblock (magic XFSB): sector_size, agcount, blocksize, feature flags
- Create `crates/fs-xfs/src/ag.rs` — AG header, AGF (free space B+tree), AGI (inode B+tree), AGFL
- Create `crates/fs-xfs/src/btree.rs` — generic B+tree traversal: short format (bmbt), leaf/node record iteration, key comparison
- Create `crates/fs-xfs/src/inode.rs` — v3 inode core: mode/uid/gid, 4 timestamps (atime/mtime/ctime/crtime), data fork format (local/extents/B+tree)
- Create `crates/fs-xfs/src/directory.rs` — shortform + block + B+tree directory (dirent format, filename hashing)
- Create `crates/fs-xfs/src/symlink.rs` — shortform and extent symlink resolution
- Add `fs-xfs = { workspace = true }` to root `Cargo.toml`

Fixtures:
- Create `testdata/fixtures/public-small/xfs-v5-basic.img` — 4 AGs, files+dirs+symlinks
- Create `testdata/fixtures/public-small/xfs-btree-dir.img` — B+tree directory (1,000+ entries)
- Create matching expected JSON in `testdata/fixtures/expected/`

Tests:
- `crates/fs-xfs/tests/superblock_tests.rs`, `ag_tests.rs`, `inode_tests.rs`, `btree_tests.rs`, `directory_tests.rs`, `fixture_tests.rs`

**Phase 3: Btrfs Reader Crate (weeks 5-7)**

Crate scaffolding:
- Create `crates/fs-btrfs/Cargo.toml`
- Create `crates/fs-btrfs/src/lib.rs` — BtrfsReader, BtrfsConfig, BtrfsError
- Create `crates/fs-btrfs/src/superblock.rs` — superblock magic (_BHRfS_M) at offsets 0x10000, 0x4000000000, 0x4000000000000; fs_tree_root, chunk_tree_root, nodesize (4/8/16K)
- Create `crates/fs-btrfs/src/btree.rs` — B-tree node traversal: header, internal/leaf nodes, key binary search (objectid, type, offset), item access
- Create `crates/fs-btrfs/src/disk_structs.rs` — key types: inode_item, dir_item, dir_index, extent_item, root_item, root_ref
- Create `crates/fs-btrfs/src/chunk_tree.rs` — chunk tree: logical→physical address mapping, stripe/device resolution
- Create `crates/fs-btrfs/src/inode.rs` — inode_item parsing, file extent mapping via extent tree
- Create `crates/fs-btrfs/src/directory.rs` — dir_item + dir_index: filename→inode lookup, hash collision handling
- Create `crates/fs-btrfs/src/subvolume.rs` — root tree enumeration, subvolume listing, default subvolume resolution
- Add `fs-btrfs = { workspace = true }` to root `Cargo.toml`

Fixtures:
- Create `testdata/fixtures/public-small/btrfs-basic.img` — 1 fs tree, files+dirs+symlinks, chunk tree
- Create `testdata/fixtures/public-small/btrfs-subvolumes.img` — @, @home, @var subvolumes with content
- Create matching expected JSON

Tests:
- `crates/fs-btrfs/tests/superblock_tests.rs`, `btree_tests.rs`, `chunk_tests.rs`, `inode_tests.rs`, `directory_tests.rs`, `subvolume_tests.rs`, `fixture_tests.rs`

**Phase 4: APFS Reader Crate (weeks 7-9)**

Crate scaffolding:
- Create `crates/fs-apfs/Cargo.toml`
- Create `crates/fs-apfs/src/lib.rs` — ApfsReader, ApfsConfig, ApfsError
- Create `crates/fs-apfs/src/container.rs` — container superblock (nx_superblock): magic NXSB, block_size, checkpoint descriptor areas (xid, type, subtype), volume count
- Create `crates/fs-apfs/src/checkpoint.rs` — checkpoint mapping: nx_checkpoint → ephemeral→physical mapping, checkpoint list enumeration
- Create `crates/fs-apfs/src/volume.rs` — volume superblock (apfs_superblock): fs_oid, block_count, case_sensitive flag, encryption info
- Create `crates/fs-apfs/src/object_map.rs` — object map: virtual OID → physical block address, snapshot support
- Create `crates/fs-apfs/src/btree.rs` — APFS B-tree: node descriptor, fixed/var-length keys, key/value iteration
- Create `crates/fs-apfs/src/inode.rs` — j_inode_val: mode/uid/gid, 4 timestamps, file extent records (j_file_extent_val), crypto state
- Create `crates/fs-apfs/src/directory.rs` — j_drec + j_drec_hashed_key: case-insensitive hashed lookup
- Create `crates/fs-apfs/src/symlink.rs` — j_symlink_val resolution
- Add `fs-apfs = { workspace = true }` to root `Cargo.toml`

Fixtures:
- Create `testdata/fixtures/public-small/apfs-basic.img` — 1 volume, files+dirs+symlinks
- Create `testdata/fixtures/public-small/apfs-multi-volume.img` — 3 volumes (System, Data, VM)
- Create `testdata/fixtures/public-small/apfs-deleted-file.img` — file deleted, content still in previous checkpoint
- Create matching expected JSON

Tests:
- `crates/fs-apfs/tests/container_tests.rs`, `volume_tests.rs`, `btree_tests.rs`, `inode_tests.rs`, `directory_tests.rs`, `checkpoint_tests.rs`, `fixture_tests.rs`

**Phase 5: HFS+ Reader Crate (weeks 9-10)**

Crate scaffolding:
- Create `crates/fs-hfsplus/Cargo.toml`
- Create `crates/fs-hfsplus/src/lib.rs` — HfsPlusReader, HfsPlusConfig, HfsPlusError
- Create `crates/fs-hfsplus/src/volume_header.rs` — signature H+/HX, block_size, special files: allocation file, catalog file, extents overflow file, attributes file, startup file
- Create `crates/fs-hfsplus/src/btree.rs` — B-tree: header node, map node, index/node records, key comparison (parent_cnid+name for catalog, fork_type+cnid for extents)
- Create `crates/fs-hfsplus/src/catalog.rs` — catalog records: folder (HFSPlusCatalogFolder), file (HFSPlusCatalogFile), thread, hard link; metadata: timestamps, permissions, owner
- Create `crates/fs-hfsplus/src/extents.rs` — extents overflow tree: fork data extents for fragmented files
- Create `crates/fs-hfsplus/src/attributes.rs` — attributes B-tree: extended attributes (com.apple.*, Finder info, resource fork location)
- Create `crates/fs-hfsplus/src/symlink.rs` — symlink resolution, hard link via cnid indirection
- Create `crates/fs-hfsplus/src/allocation.rs` — allocation bitmap for free block tracking
- Add `fs-hfsplus = { workspace = true }` to root `Cargo.toml`

Fixtures:
- Create `testdata/fixtures/public-small/hfsplus-basic.img` — files+dirs+symlinks+hard links
- Create `testdata/fixtures/public-small/hfsplus-attributes.img` — extended attributes, Finder info
- Create matching expected JSON

Tests:
- `crates/fs-hfsplus/tests/volume_header_tests.rs`, `btree_tests.rs`, `catalog_tests.rs`, `extents_tests.rs`, `attributes_tests.rs`, `fixture_tests.rs`

**Phase 6: Multi-OS Probe and Deleted Recovery (weeks 10-12)**

Multi-OS probe:
- Create `crates/app-services/src/multi_os_probe.rs` — GPT parsing (protective MBR + GPT header + partition entries), partition type GUID→OS mapping, filesystem detection (superblock magic at sector offset per type), combined MultiOSProbeDto
- Create `crates/transport/src/dto/multi_os.rs` — MultiOSProbeDto, PartitionDto, DetectedFilesystemDto
- Update `frontend/src/types/models.ts`
- Create `apps/desktop/src-tauri/src/commands/multi_os_commands.rs` — probe_disk
- Register in `apps/desktop/src-tauri/src/lib.rs`

Deleted recovery:
- Create `crates/fs-ext4/src/journal.rs` — jbd2 journal replay: superblock (magic 0xC03B3998), descriptor blocks, revoke blocks, commit blocks; replay transactions to recover deleted inode+directory blocks
- Create `crates/fs-apfs/src/checkpoint_rewind.rs` — enumerate previous checkpoints, reconstruct ephemeral→physical from older checkpoint, resolve deleted file content blocks
- Create `crates/app-services/src/deleted_recovery.rs` — unified recovery service: FS type dispatch, recovery job with progress events, recovered FileEntry generation
- Add `recover_deleted_files` to `apps/desktop/src-tauri/src/commands/multi_os_commands.rs`

Fixtures:
- Create `testdata/fixtures/public-small/multi-os-gpt.img` — GPT: EFI System + NTFS + ext4 + APFS + swap
- Create `testdata/fixtures/public-small/ext4-journal-deleted.img` — ext4 with journal containing 20 file-delete transactions
- Create `testdata/fixtures/public-small/apfs-checkpoint-deleted.img` — APFS with 20 files deleted, available in older checkpoint
- Create matching expected JSON for probe and recovery fixtures

Tests:
- `crates/fs-ext4/tests/journal_tests.rs` — journal superblock, transaction replay, recovery recall >= 80%
- `crates/fs-apfs/tests/checkpoint_rewind_tests.rs` — checkpoint resolution, recovery recall >= 80%
- `crates/app-services/tests/multi_os_probe_tests.rs` — GPT with 5 partitions all correctly identified
- `crates/app-services/tests/deleted_recovery_tests.rs` — ext4 and APFS combined recovery

**Phase 7: Frontend Multi-OS Partition Visualization (weeks 12-13)**

- Create `frontend/src/components/evidence/MultiOSDiskView.tsx` — GPT disk visualization: partition blocks with OS icons (Windows/Linux/macOS), filesystem labels, click-to-explore per partition
- Create `frontend/src/app/pages/MultiOSExplorer.tsx` — multi-OS partition explorer page
- Create `frontend/src/features/multi-os/hooks.ts` — useMultiOSProbe, useDeletedRecovery hooks
- Create `frontend/src/lib/api/multi-os.ts` — probeDisk, recoverDeletedFiles API
- Add route in `frontend/src/app/routes.tsx`
- Add mock data in `frontend/src/lib/api/mock-data.ts`

**Phase 8: Integration Tests and Governance (weeks 13-14)**

- Create `crates/testing/src/multi_os_fixtures.rs` — shared test helpers
- Create full-disk integration tests per FS: `crates/fs-ext4/tests/integration_full_disk.rs`, `crates/fs-xfs/tests/integration_full_disk.rs`
- Add platform discriminant to `crates/catalog/src/graph.rs` — File nodes carry OS/platform tag (Windows/Linux/macOS)
- Create `docs/v4-multi-os-filesystems.md` — reader architecture, trust framework levels per FS
- Create `scripts/check-fs-reader-trust-framework.ps1` — each FS reader has public-small fixture + expected JSON + guarantee level
- Update `CLAUDE.md` and `AGENTS.md` with new fs-* crate descriptions
- Run full V3 regression after FS reader registration

#### Test Matrix

| # | Test Name | Scenario | Expected Result | Phase |
|---|-----------|----------|-----------------|-------|
| FS-01 | `test_ext4_superblock_magic` | Ext4 superblock at offset 1024 | magic 0xEF53, block_size, inode_size parsed | P1 |
| FS-02 | `test_ext4_feature_flags` | Ext4 feature compat/incompat/ro_compat | extent, dir_index, journal, flex_bg, 64bit correctly read | P1 |
| FS-03 | `test_ext4_extent_file_read` | File stored in extent tree | correct bytes, all extents traversed | P1 |
| FS-04 | `test_ext4_indirect_block_file` | File using indirect block chain | correct bytes via double/triple indirection | P1 |
| FS-05 | `test_ext4_htree_lookup` | Lookup filename in HTree-indexed dir | correct inode returned | P1 |
| FS-06 | `test_ext4_htree_enumerate` | Enumerate 100+ entries in HTree dir | all entries, no duplicates | P1 |
| FS-07 | `test_ext4_symlink_fast` | Read fast symlink (< 60 bytes in inode) | correct target path | P1 |
| FS-08 | `test_ext4_symlink_slow` | Read slow symlink (data block) | correct target path | P1 |
| FS-09 | `test_ext4_public_fixture_extent` | public-small ext4-extent-file.img | all guaranteed fields match expected JSON | P1 |
| FS-10 | `test_ext4_public_fixture_htree` | public-small ext4-htree-dir.img | all guaranteed fields match expected JSON | P1 |
| FS-11 | `test_ext4_evidence_reader_trait` | Ext4Reader implements EvidenceReader | all trait methods callable with valid returns | P1 |
| FS-12 | `test_xfs_superblock_v5` | XFS v5 superblock (XFSB magic) | magic, sector_size, agcount, blocksize parsed | P2 |
| FS-13 | `test_xfs_ag_iteration` | Iterate 4 AGs on XFS v5 image | all AGs enumerated, free space correct | P2 |
| FS-14 | `test_xfs_inode_v3_extents` | XFS v3 inode with extent B+tree | mode, uid/gid, 4 timestamps, extents resolved | P2 |
| FS-15 | `test_xfs_btree_directory` | Lookup in B+tree directory (1,000+ entries) | correct inode for filename | P2 |
| FS-16 | `test_xfs_public_fixture_basic` | public-small xfs-v5-basic.img | all guaranteed fields match expected JSON | P2 |
| FS-17 | `test_btrfs_superblock_magic` | Btrfs superblock at standard offsets | valid at 0x10000, 0x4000000000, 0x4000000000000 | P3 |
| FS-18 | `test_btrfs_chunk_tree_translate` | Logical→physical address translation | correct physical offset for known logical addr | P3 |
| FS-19 | `test_btrfs_inode_with_extents` | inode_item with extent mapping | file size, mode, uid/gid, extents resolved | P3 |
| FS-20 | `test_btrfs_dir_item_index` | dir_item + dir_index: filename→inode | correct inode for filename, hash collision OK | P3 |
| FS-21 | `test_btrfs_subvolume_listing` | Root tree: list subvolumes | @, @home, @var all listed | P3 |
| FS-22 | `test_btrfs_subvolume_content` | List files in @home subvolume | correct file listing, not mixed with other subvols | P3 |
| FS-23 | `test_btrfs_public_fixture_basic` | public-small btrfs-basic.img | all guaranteed fields match expected JSON | P3 |
| FS-24 | `test_apfs_container_superblock` | APFS container (NXSB magic) | block_size, checkpoint desc areas, volume count | P4 |
| FS-25 | `test_apfs_volume_listing` | List all volumes in container | volume names and fs_oid values correct | P4 |
| FS-26 | `test_apfs_inode_parse` | APFS inode: mode, timestamps, extents | all fields parsed, file content reachable | P4 |
| FS-27 | `test_apfs_directory_case_insensitive` | Lookup filename (case-insensitive) | correct inode via hashed key | P4 |
| FS-28 | `test_apfs_checkpoint_enumeration` | Enumerate container checkpoints | current+previous checkpoints listed | P4 |
| FS-29 | `test_apfs_public_fixture_basic` | public-small apfs-basic.img | all guaranteed fields match expected JSON | P4 |
| FS-30 | `test_apfs_public_fixture_multi_vol` | public-small apfs-multi-volume.img | 3 volumes with correct content | P4 |
| FS-31 | `test_hfsplus_signature` | HFS+ volume header (H+/HX) | block_size, 5 special files resolved | P5 |
| FS-32 | `test_hfsplus_catalog_file_record` | Catalog B-tree: file record | CNID, timestamps, permissions, data fork location | P5 |
| FS-33 | `test_hfsplus_catalog_folder_thread` | Folder record + thread record | correct parent-child CNID relationships | P5 |
| FS-34 | `test_hfsplus_hard_link_indirection` | Hard link → CNID → file content | both paths resolve to same data | P5 |
| FS-35 | `test_hfsplus_extended_attributes` | Read FinderInfo + com.apple.xattrs | attribute data returned correctly | P5 |
| FS-36 | `test_hfsplus_public_fixture_basic` | public-small hfsplus-basic.img | all guaranteed fields match expected JSON | P5 |
| FS-37 | `test_multi_os_probe_gpt_5_partitions` | GPT: EFI+NTFS+ext4+APFS+swap | all 5 identified with correct OS/FS type | P6 |
| FS-38 | `test_multi_os_probe_mbr_fallback` | MBR disk with NTFS+ext4 | partitions detected via protective MBR | P6 |
| FS-39 | `test_ext4_journal_replay_recover` | Journal with 20 known-deleted files | recall >= 80% (>= 16 recovered) | P6 |
| FS-40 | `test_apfs_checkpoint_rewind_recover` | Previous checkpoint with 20 deleted files | recall >= 80% (>= 16 recovered) | P6 |
| FS-41 | `test_platform_discriminant_on_graph` | Import ext4/XFS/APFS/HFS+ evidence | File nodes tagged OS=Linux or OS=macOS | P8 |
| FS-42 | `test_fs_reader_trust_framework_guard` | check-fs-reader-trust-framework.ps1 | all 5 FS readers have fixture+JSON+guarantee_level | P8 |
| FS-43 | `test_v3_regression_after_fs_readers` | Full V3 suite after all FS readers registered | 1,345 Rust + 228 frontend: 100% pass | P8 |

#### Acceptance Criteria

- **ext4**: all 3 public-small fixtures (extent file, HTree dir, symlink) parse against expected JSON on guaranteed fields. EvidenceReader trait fully implemented. Indirect block chain resolution works.
- **XFS**: xfs-v5-basic and xfs-btree-dir fixtures parse against expected JSON. v3 inode with 4 timestamps. B+tree directory with 1,000+ entries.
- **Btrfs**: btrfs-basic and btrfs-subvolumes fixtures parse against expected JSON. Chunk tree logical→physical translation correct. Subvolume isolation: listing @home does not include @ files.
- **APFS**: apfs-basic and apfs-multi-volume fixtures parse against expected JSON. Case-insensitive directory lookup via hashed keys. Checkpoint enumeration returns current+previous.
- **HFS+**: hfsplus-basic and hfsplus-attributes fixtures parse against expected JSON. Hard link indirection resolves. Extended attributes readable.
- **Multi-OS probe**: GPT disk with EFI+NTFS+ext4+APFS partitions correctly identifies all 4 file systems + EFI. MBR fallback works.
- **Deleted recovery**: ext4 journal replay recall >= 80% (16/20) on synthetic fixture. APFS checkpoint rewind recall >= 80% (16/20) on synthetic fixture. Recovered files appear as FileEntry nodes in graph.
- **Platform discriminant**: All File nodes from Linux FS carry `platform=Linux`. All macOS FS nodes carry `platform=macOS`. Existing NTFS nodes unaffected.
- **Trust framework**: Each of 5 FS crates has at minimum 1 public-small fixture + expected JSON + documented guarantee level.
- **Frontend**: MultiOSDiskView renders GPT layout with correct partition sizes, OS icons, and FS labels. Clicking a partition opens its file browser.
- **Performance**: Each FS reader parses its public-small fixture within 2 seconds. Multi-OS probe completes within 1 second for GPT disks up to 2 TB.
- **V3 regression**: All 1,345 Rust + 228 frontend tests pass after FS reader registration. No impact on existing E01/NTFS import.

#### Expected Results

**After Phase 1**: `cargo test -p fs-ext4` shows all tests pass. Importing a raw ext4 disk image via the evidence import flow creates FileEntry nodes in the graph. File content is readable through the evidence media protocol.

**After Phase 2**: `cargo test -p fs-xfs` passes. XFS v5 images import successfully. The EvidenceGraph shows File nodes for XFS volumes alongside existing NTFS nodes.

**After Phase 3**: `cargo test -p fs-btrfs` passes. Btrfs subvolumes appear as separate root directories in the file browser. Chunk tree translation handles multi-device basic layout.

**After Phase 4**: `cargo test -p fs-apfs` passes. APFS containers show as a parent node with child volumes. Case-insensitive file search works on APFS volumes.

**After Phase 5**: `cargo test -p fs-hfsplus` passes. HFS+ evidence imports with Finder metadata visible as extended attributes. Hard links resolve to the same file content.

**After Phase 6**: Importing a multi-OS GPT disk image shows the partition visualization. The user selects partitions to import. Running deleted recovery on ext4 or APFS images surfaces recovered files with a "recovered" badge in the file browser.

**After Phase 7**: The MultiOSExplorer page (`/evidence/multi-os`) displays the GPT partition map. Each partition shows its OS icon (Windows/Linux/Apple) and filesystem type. Recovered files appear in a separate "Deleted Recovery" section.

**After Phase 8**: Documentation complete in `docs/v4-multi-os-filesystems.md`. Trust framework guard passes. Full V3 regression confirms zero impact. 5 new filesystem crates integrated into the workspace.

---

### Stage V4-3: AI-Assisted Investigation — **DEFERRED**

> Per product decision 2026-06-17, local LLM integration is deferred. AI assistance (notebook summarization, lead narratives, gap analysis, NL search) will be revisited when local inference performance and model quality meet investigative standards. See section 8 V5 Directions for re-evaluation timeline.
**In scope**: Notebook entry summarization (condense long entries into structured findings with confidence indicators), lead narrative generation (explain why a set of leads is interesting in plain language with evidence citations), evidence gap analysis (identify missing evidence types based on investigation templates), natural language search query assistance (convert "find all executables downloaded in the last week" to structured search).

**Deferred**: Full AI-driven investigation automation, evidence tampering detection, cloud-based AI integration.

#### Phase Tasks

> **DEFERRED** — All tasks below describe the target implementation when this stage is un-deferred. No active development is scheduled. See section 8 V5 Directions for re-evaluation criteria.

**Phase 1: Local LLM Integration (weeks 1-3)**

Crate scaffolding:
- Create `crates/ai-core/Cargo.toml` — workspace member, depends on infrastructure (logging, config)
- Create `crates/ai-core/src/lib.rs` — AiConfig, AiEngine, AiError, ModelInfo
- Create `crates/ai-core/src/engine.rs` — model loading (GGUF format via llama.cpp bindings), inference engine with configurable context window (default 4096 tokens), GPU offload detection (Vulkan/Metal/CUDA), model warmup and caching
- Create `crates/ai-core/src/prompts.rs` — prompt template registry: summarization, narrative, gap analysis, NL-to-search; each with system prompt + few-shot examples
- Create `crates/ai-core/src/privacy.rs` — network sandbox: block all outbound network calls from AI thread, audit log of all data passed to inference engine, token-level PII detection on output
- Add `ai-core = { workspace = true }` to root `Cargo.toml`

Model management:
- Create `crates/ai-core/src/model_store.rs` — model download/update from HuggingFace (manual, not auto), model verification (SHA256), model version tracking
- Add `models/` directory to `.gitignore`
- Create `docs/v4-ai-models.md` — recommended models (e.g., Phi-3-mini, Llama-3.2-3B, Qwen2.5-7B), size/quality tradeoffs, benchmark data

DTO:
- Create `crates/transport/src/dto/ai.rs` — AiModelInfoDto, AiInferenceRequestDto, AiInferenceResponseDto, AiStreamChunkDto
- Update `crates/transport/src/dto/mod.rs`
- Update `frontend/src/types/models.ts`

Tests:
- `crates/ai-core/tests/engine_tests.rs` — model loading, basic inference, context window handling
- `crates/ai-core/tests/privacy_tests.rs` — network sandbox enforcement, PII detection, audit log integrity

**Phase 2: Notebook Summarization Pipeline (weeks 3-5)**

Engine:
- Create `crates/app-services/src/ai_service.rs` — AiService: summarize_notebook_entries, generate_lead_narrative, analyze_evidence_gaps, translate_nl_query
- Create `crates/ai-core/src/summarization.rs` — summarization pipeline: chunk long entries (> 1000 tokens) into overlapping segments, generate per-chunk summary, merge chunk summaries into structured finding JSON (title, key_facts[], entities[], confidence, recommended_actions[])
- Create `crates/ai-core/src/output_parser.rs` — structured output parsing: JSON schema validation on AI output, retry with error correction prompt on parse failure (max 3 attempts)

DTO:
- Add to `crates/transport/src/dto/ai.rs`: NotebookSummarizationRequestDto (entry_ids, output_style), SummarizationResultDto (findings per entry, tokens_used, elapsed_ms)
- Update `frontend/src/types/models.ts`

Commands:
- Create `apps/desktop/src-tauri/src/commands/ai_commands.rs` — summarize_notebook, generate_narrative, analyze_gaps, nl_search
- Register in `apps/desktop/src-tauri/src/lib.rs`

Frontend:
- Create `frontend/src/components/ai/SummarizeButton.tsx` — button on notebook entry, shows progress spinner during inference
- Create `frontend/src/components/ai/FindingCard.tsx` — structured finding display with confidence badge, key facts list, entity links

Tests:
- `crates/ai-core/tests/summarization_tests.rs` — 500-word entry produces valid finding JSON, chunk merging preserves fact ordering, confidence indicator within 0.0-1.0
- `crates/app-services/tests/ai_service_summarization_tests.rs` — end-to-end: entry IDs in → findings out, error handling on empty entries

**Phase 3: Lead Narrative Generation (weeks 5-7)**

Engine:
- Create `crates/ai-core/src/narrative.rs` — narrative generator: input is a lead cluster (lead_id, evidence_refs[], entity_refs[], artifact_refs[]), build context from evidence graph subgraph, generate plain-language narrative with: (1) what happened, (2) why it matters, (3) supporting evidence citations (artifact IDs, file paths, timestamps), (4) confidence assessment

DTO:
- Add to `crates/transport/src/dto/ai.rs`: LeadNarrativeRequestDto (lead_ids, detail_level), LeadNarrativeDto (narrative_text, citations[], confidence)
- Add to `crates/transport/src/dto/ai.rs`: LeadNarrativeStreamChunkDto (for streaming display)

Commands:
- Add `generate_lead_narrative` (with optional streaming via Tauri events) to ai_commands.rs

Frontend:
- Create `frontend/src/components/ai/NarrativeView.tsx` — narrative text with inline citation highlights (clickable → evidence viewer), confidence indicator, streaming text animation
- Create `frontend/src/app/pages/AiAssistant.tsx` — AI assistant page container

Tests:
- `crates/ai-core/tests/narrative_tests.rs` — 5-lead cluster produces narrative citing >= 3 specific evidence items, confidence in valid range
- `crates/app-services/tests/ai_service_narrative_tests.rs` — end-to-end with real lead cluster data

**Phase 4: Evidence Gap Analysis (weeks 7-8)**

Engine:
- Create `crates/ai-core/src/gap_analysis.rs` — gap analyzer: load investigation template (e.g., ransomware, insider threat, data exfiltration), enumerate expected artifact types per template, check which artifact types are present in current case evidence, compare against template requirements, generate gap report: missing artifact types, why each matters, suggested acquisition methods

Template registry:
- Create `crates/ai-core/src/templates/` directory
- Create `crates/ai-core/src/templates/mod.rs` — template loader (YAML/JSON format)
- Create `crates/ai-core/src/templates/ransomware.tmpl` — expected artifacts: Prefetch, EVTX (Security 4688, System 7045), Registry (Run keys, Services), LNK files, browser history, email (ransom note)
- Create `crates/ai-core/src/templates/insider_threat.tmpl`
- Create `crates/ai-core/src/templates/data_exfiltration.tmpl`

DTO:
- Add to `crates/transport/src/dto/ai.rs`: EvidenceGapAnalysisRequestDto (template_name, case_id), EvidenceGapAnalysisDto (template_name, missing_artifacts[], present_artifacts[], overall_completeness_pct, recommendations[])

Tests:
- `crates/ai-core/tests/gap_analysis_tests.rs` — ransomware template correctly identifies missing EVTX/Prefetch when absent, overall completeness calculation, template loading from YAML

**Phase 5: NL-to-Structured-Search Query Engine (weeks 8-10)**

Engine:
- Create `crates/ai-core/src/nl_search.rs` — NL-to-structured translator: parse natural language query intent ("find all executables downloaded in the last week"), extract search dimensions (file_type: executable, time_range: last 7 days, action: downloaded), map dimensions to structured search DTO fields, generate SearchRequestDto
- Create `crates/ai-core/src/nl_search_patterns.rs` — pattern library: 20+ common forensic query patterns (find files by type/time/owner, find communications by sender/recipient/date, find logins by user/host/time, find registry modifications, find deleted files)

DTO:
- Add to `crates/transport/src/dto/ai.rs`: NLQueryRequestDto, NLQueryResponseDto (structured_filters, confidence, alternative_interpretations[])

Commands:
- Add `nl_search` to ai_commands.rs — returns both structured search DTO and AI interpretation for user review

Frontend:
- Create `frontend/src/components/ai/NLSearchBar.tsx` — natural language input in search bar, "AI" toggle, shows structured filter preview before executing search
- Integrate into existing search page at `frontend/src/app/pages/Search.tsx`

Tests:
- `crates/ai-core/tests/nl_search_tests.rs` — 5 query patterns: "find executables downloaded last week", "show all emails from user@corp.com", "list failed logins on workstation-01", "find files modified after 2024-06-01", "show registry changes made by SYSTEM"
- Each test verifies the structured SearchRequestDto fields match expected values

**Phase 6: Frontend AI Panel (weeks 10-11)**

- Create `frontend/src/app/pages/AiAssistant.tsx` — tabbed AI assistant: Summaries tab, Narratives tab, Gap Analysis tab, NL Search tab
- Create `frontend/src/components/ai/AiPanel.tsx` — slide-out panel from right side, model status indicator (loaded/loading/error), token usage meter
- Create `frontend/src/components/ai/ModelSettings.tsx` — model selection dropdown, context window slider, GPU offload toggle
- Add route `/ai` in `frontend/src/app/routes.tsx`
- Add mock data for all AI endpoints
- Create `frontend/src/features/ai/hooks.ts` — useSummarization, useNarrative, useGapAnalysis, useNLSearch hooks
- Create `frontend/src/lib/api/ai.ts` — all AI API client functions

**Phase 7: Privacy Guard and Governance (weeks 11-12)**

- Create `crates/ai-core/src/privacy_guard.rs` — runtime privacy enforcement: network firewall at thread level (deny all outbound), data-at-rest encryption for model weights, in-memory-only processing (no evidence data written to disk outside case DB), audit log of every inference request (what data was passed, what was returned, token count, latency)
- Create `scripts/check-ai-privacy-guard.ps1` — static analysis: verify no network imports (reqwest, hyper, ureq) in ai-core dependency tree; verify evidence data never written to temp files
- Create `docs/v4-ai-privacy.md` — privacy architecture, data flow diagram, threat model (model exfiltration, prompt injection, data leakage)
- Run privacy guard in CI: network monitor on test runs confirms zero outbound connections during AI tests

#### Test Matrix

> **DEFERRED** — Tests below describe the target test suite when un-deferred.

| # | Test Name | Scenario | Expected Result | Phase |
|---|-----------|----------|-----------------|-------|
| AI-01 | `test_model_load_gguf` | Load Phi-3-mini GGUF model | load time < 30s, inference produces text | P1 |
| AI-02 | `test_model_context_window` | Inference with input near context limit | no truncation, output coherent | P1 |
| AI-03 | `test_privacy_no_network` | Run AI inference; monitor network | zero outbound connections | P1 |
| AI-04 | `test_privacy_pii_detection` | Output contains email/SSN/IP patterns | PII flagged in audit log | P1 |
| AI-05 | `test_summarize_500_word_entry` | Notebook entry: 500 words | valid finding JSON: title, key_facts, entities, confidence | P2 |
| AI-06 | `test_summarize_long_entry_chunking` | Notebook entry: 2,500 words (> context window) | chunked, merged, no information loss | P2 |
| AI-07 | `test_summarize_empty_entry` | Notebook entry: 0 words | handles gracefully, no crash | P2 |
| AI-08 | `test_summarize_output_parse_retry` | AI produces malformed JSON | retry up to 3x, fallback to plain text | P2 |
| AI-09 | `test_narrative_5_lead_cluster` | Lead cluster: 5 leads with evidence refs | narrative cites >= 3 specific artifact IDs or file paths | P3 |
| AI-10 | `test_narrative_confidence_assessment` | Lead cluster with mixed-quality evidence | confidence assessment in 0.0-1.0 range, reflects evidence strength | P3 |
| AI-11 | `test_narrative_streaming` | Streaming narrative generation | Tauri events emitted as chunks arrive, UI renders progressively | P3 |
| AI-12 | `test_gap_analysis_ransomware_template` | Case with EVTX+Registry, no Prefetch | missing Prefetch identified, completeness < 100% | P4 |
| AI-13 | `test_gap_analysis_full_evidence` | Case with all ransomware template artifacts | completeness = 100%, no missing artifacts | P4 |
| AI-14 | `test_gap_analysis_unknown_template` | Request analysis with non-existent template | graceful error, template list returned | P4 |
| AI-15 | `test_nl_search_executables_downloaded` | "find executables downloaded in the last week" | file_type=executable, action=downloaded, time=last_7d | P5 |
| AI-16 | `test_nl_search_emails_from_user` | "show all emails from user@corp.com" | artifact_type=email, sender=user@corp.com | P5 |
| AI-17 | `test_nl_search_failed_logins` | "list failed logins on workstation-01" | event_type=login_failed, host=workstation-01 | P5 |
| AI-18 | `test_nl_search_files_modified_after` | "find files modified after 2024-06-01" | time_start=2024-06-01, action=modified | P5 |
| AI-19 | `test_nl_search_registry_by_system` | "show registry changes made by SYSTEM" | artifact_type=registry, user=SYSTEM | P5 |
| AI-20 | `test_nl_search_ambiguous_query` | "find stuff" | returns alternative_interpretations[], asks for clarification | P5 |
| AI-21 | `test_ai_panel_model_status` | Frontend: open AI panel with model loaded | status shows "Ready", model name, context size | P6 |
| AI-22 | `test_ai_panel_token_usage` | Frontend: run 3 summarizations | token meter shows cumulative usage, resets on model reload | P6 |
| AI-23 | `test_privacy_guard_script` | check-ai-privacy-guard.ps1 | no network imports in ai-core dep tree | P7 |
| AI-24 | `test_privacy_evidence_no_temp_write` | Run summarization on case evidence | no evidence data written outside case DB | P7 |

#### Acceptance Criteria

> **DEFERRED** — Criteria below are target thresholds when un-deferred.

- **Model loading**: GGUF model loads within 30 seconds; inference produces coherent text for forensic domain prompts; context window configurable up to 8192 tokens.
- **Notebook summarization**: 500+ word entries produce valid structured finding JSON with title, key_facts (>= 3), entities (>= 1 when present), and confidence (0.0-1.0). Chunking works for entries exceeding context window. Malformed JSON output retries up to 3 times, then falls back to plain text.
- **Lead narrative generation**: 5-lead cluster produces narrative with >= 3 specific evidence citations (artifact IDs, file paths, or timestamps). Confidence assessment calibrated to evidence strength (corroborated evidence → higher confidence). Streaming delivery via Tauri events for progressive UI rendering.
- **Evidence gap analysis**: Correctly identifies missing artifact types per investigation template. Completeness percentage calculated as present_count / total_expected. Supports >= 3 templates (ransomware, insider threat, data exfiltration).
- **NL search**: Converts >= 5 distinct query patterns to correct structured SearchRequestDto fields. Returns alternative_interpretations for ambiguous queries. Preview of structured filters shown to user before execution.
- **Privacy guarantee**: Zero network calls from AI subsystem (verified by network monitor in CI). PII detected in AI output is flagged in audit log. No evidence data written to temporary files outside case database. Model weights verified by SHA256 before loading.
- **Frontend UX**: AI panel shows model status (Ready/Loading/Error). Token usage meter tracks per-session usage. Summarization completes within 30 seconds for 500-word entry.
- **Guard script**: `check-ai-privacy-guard.ps1` passes — zero network imports in ai-core dependency tree.

#### Expected Results

> **DEFERRED** — Results below describe target user experience when un-deferred.

**After Phase 1**: Running `cargo test -p ai-core` shows model loading and basic inference. The AiEngine can be configured with a model path and produces text from prompts. The privacy sandbox is active — no network calls are possible from the AI thread.

**After Phase 2**: Clicking "Summarize" on a notebook entry shows a progress spinner, then displays a structured finding card with title, key facts, entities, and a confidence badge. Long entries are automatically chunked and merged.

**After Phase 3**: On the lead detail page, clicking "Generate Narrative" produces a plain-language explanation of the lead cluster in the AI Assistant panel. The narrative includes inline citations that link to the evidence viewer. Text streams in progressively.

**After Phase 4**: Opening the Gap Analysis tab and selecting "Ransomware" template shows a checklist: green checks for present artifacts, red Xs for missing ones, and a completeness percentage. Recommendations suggest how to acquire missing evidence types.

**After Phase 5**: In the search bar, toggling "AI" mode allows typing "find executables downloaded in the last week". A structured filter preview appears below the search bar before executing. The user can edit the structured filters before searching.

**After Phase 6**: The AI Assistant page (`/ai`) has tabs for Summaries, Narratives, Gap Analysis, and NL Search. The model status indicator in the top-right shows "Ready: Phi-3-mini (4K ctx)". Token usage is tracked per session.

**After Phase 7**: The `check-ai-privacy-guard.ps1` script passes in CI. Privacy documentation is complete with architecture diagram and threat model. Zero network traffic confirmed during all AI test runs.

---

### Stage V4-4: Exchange, Chain-of-Custody & Platform Maturity（25 分，12 周）

#### Objective
Enable professional inter-tool exchange, cryptographically-verified chain-of-custody, and platform maturity for production deployment.

#### Stage boundaries
**In scope**: STIX 2.1 export (indicators, observed-data, relationships from correlation leads), CASE/UCO export (forensic case model), digital signatures on exports (Ed25519), chain-of-custody log with Merkle tree verification, case export bundle with selective redaction, inter-tool import (Autopsy XML report, Plaso timeline CSV), production hardening (installer signing, update mechanism, crash reporting).

#### Phase Tasks

**Phase 1: STIX 2.1 Export Engine (weeks 1-3)**

Crate scaffolding:
- Create `crates/exchange/Cargo.toml` — workspace member, depends on app-services, domain, transport
- Create `crates/exchange/src/lib.rs` — ExchangeService, ExportConfig, ImportConfig, ExchangeError
- Create `crates/exchange/src/stix/mod.rs` — StixExporter: correlation_leads_to_stix_bundle
- Create `crates/exchange/src/stix/mapper.rs` — domain→STIX mappers: Entity → Identity/SCO, File → File SCO, NetworkEndpoint → IPv4Addr/IPv6Addr/DomainName SCO, Relationship → Relationship SRO, Artifact → ObservedData SDO, Lead → Indicator SDO
- Create `crates/exchange/src/stix/serializer.rs` — STIX 2.1 JSON serialization per OASIS spec: bundle wrapper, spec_version 2.1, type-specific required fields, custom properties under extensions dictionary
- Create `crates/exchange/src/stix/validator.rs` — STIX schema validation: validate required fields per object type, validate reference integrity (SRO source_ref/target_ref exist), validate timestamp format (RFC 3339), validate observable object structure (Cyber Observable Core)
- Add `exchange = { workspace = true }` to root `Cargo.toml`

DTO:
- Add to `crates/transport/src/dto/exchange.rs`: StixExportRequestDto (case_id, include_entities, include_indicators, include_observables, observed_data_refs[], relationship_refs[]), StixExportResultDto (bundle_json, object_counts, validation_errors[], validation_passed)
- Update `frontend/src/types/models.ts`

Commands:
- Create `apps/desktop/src-tauri/src/commands/exchange_commands.rs` — export_stix, export_case, import_autopsy, import_plaso, get_custody_log, verify_signature, redact_and_export
- Register in `apps/desktop/src-tauri/src/lib.rs`

Tests:
- `crates/exchange/tests/stix_mapper_tests.rs` — Person→Identity (name, email fields), File→File SCO (name, hashes, size, created/modified), IP→IPv4Addr (value), relationship→Relationship SRO (relationship_type=CommunicatesWith → stix mapping), indicator→Indicator (pattern, pattern_type=stix)
- `crates/exchange/tests/stix_serializer_tests.rs` — bundle JSON output matches STIX 2.1 spec format, all required fields present, custom properties in extensions
- `crates/exchange/tests/stix_validator_tests.rs` — valid bundle passes, missing required field flagged, broken reference flagged
- `crates/app-services/tests/exchange_service_stix_tests.rs` — end-to-end: case with leads/entities → STIX bundle, validation passes

**Phase 2: CASE/UCO Export Engine (weeks 3-5)**

Engine:
- Create `crates/exchange/src/case/mod.rs` — CaseExporter: full investigation case → CASE/UCO JSON-LD
- Create `crates/exchange/src/case/uco_mapper.rs` — domain→UCO mappers: Case → Investigation (uco-core:Investigation), File → File (uco-observable:File), FileEntry → ObservableObject with HashFacet, FileFacet, ContentDataFacet; Entity → Identity (uco-identity:Identity), Relationship → Relationship (uco-core:Relationship), Timeline events → uco-observable:ObservableObject with TimestampFacet
- Create `crates/exchange/src/case/serializer.rs` — CASE JSON-LD serialization: @context with UCO namespace, @type fields, @id IRI generation (urn:uuid:...), facet-based property embedding
- Create `crates/exchange/src/case/validator.rs` — UCO schema validation: required facets per object type, reference integrity, namespace conformance

DTO:
- Add to `crates/transport/src/dto/exchange.rs`: CaseExportRequestDto (case_id, format (STIX/CASE/both), include_redactions), CaseExportResultDto (stix_json, case_jsonld, object_counts, validation_errors[], output_size_bytes)

Tests:
- `crates/exchange/tests/case_uco_mapper_tests.rs` — File→uco-observable:File with facets, Entity→uco-identity:Identity, Relationship→uco-core:Relationship
- `crates/exchange/tests/case_serializer_tests.rs` — JSON-LD output valid, @context includes UCO namespace, @type matches UCO class
- `crates/exchange/tests/case_validator_tests.rs` — valid CASE output passes validation, missing facet flagged

**Phase 3: Digital Signature and Chain-of-Custody (weeks 5-7)**

Signing infrastructure:
- Create `crates/exchange/src/signing.rs` — Ed25519 key generation (keypair via ed25519-dalek or ring), key serialization (PEM format), key storage (case DB, encrypted at rest), sign_export_bundle (sign entire export JSON), verify_signature (public key + bundle + signature)
- Create `crates/exchange/src/signing/key_store.rs` — key management: generate_keypair, import_public_key, list_keys, revoke_key; key metadata (created_at, purpose, fingerprint)
- Add signing dependency to root `Cargo.toml` workspace: `ed25519-dalek = "2"`

Chain-of-custody:
- Create `crates/exchange/src/custody.rs` — ChainOfCustodyLog: append-only log of custody events, each event has (timestamp, event_type, actor, description, evidence_refs, previous_entry_hash), Merkle tree construction over all entries, Merkle proof generation for any entry, tamper detection (verify Merkle root integrity)
- Create `crates/exchange/src/custody/merkle.rs` — Merkle tree: build from custody entries (SHA-256 leaf hashes), root hash computation, inclusion proof (sibling hashes from leaf to root), proof verification (recompute root from leaf + proof)
- Create `crates/exchange/src/custody/event_types.rs` — event types: EvidenceAdded, EvidenceRemoved, CaseExported, CaseArchived, CustodyTransferred, AnalysisStarted, AnalysisCompleted

Persistence:
- Create `crates/persistence-sqlite/migrations/0034_signing_keys.sql` — signing_keys table (id, key_type, public_key_pem, encrypted_private_key, created_at, is_active, fingerprint)
- Create `crates/persistence-sqlite/migrations/0035_custody_log.sql` — custody_log table (id, timestamp, event_type, actor, description, evidence_refs JSON, previous_entry_hash, merkle_root_at_time)
- Create `crates/persistence-sqlite/src/signing_repo.rs` — SigningRepo: CRUD for keys
- Create `crates/persistence-sqlite/src/custody_repo.rs` — CustodyRepo: append_event, get_log, get_merkle_proof, verify_log_integrity

DTO:
- Add to `crates/transport/src/dto/exchange.rs`: DigitalSignatureDto, SigningKeyDto, ChainOfCustodyEntryDto, MerkleProofDto, LogIntegrityReportDto
- Update `frontend/src/types/models.ts`

Commands:
- Add to exchange_commands.rs: generate_signing_key, list_signing_keys, sign_export, verify_export_signature, get_custody_log, get_merkle_proof, verify_log_integrity, append_custody_event

Tests:
- `crates/exchange/tests/signing_tests.rs` — key generation, sign+verify roundtrip, wrong public key fails verification, tampered bundle fails verification
- `crates/exchange/tests/custody_tests.rs` — append events, Merkle root consistent, inclusion proof verifies, tampered entry detected via root mismatch
- `crates/persistence-sqlite/tests/signing_repo_tests.rs` — key CRUD, active key isolation
- `crates/persistence-sqlite/tests/custody_repo_tests.rs` — append-only enforcement, log retrieval, proof generation

**Phase 4: Case Export Bundle with Redaction (weeks 7-8)**

Engine:
- Create `crates/exchange/src/bundle.rs` — ExportBundle: combine STIX + CASE/UCO + metadata manifest + digital signature into a single ZIP/tar archive; manifest.json with manifest_version, case_id, export_timestamp, file_entries_count, artifact_count, entity_count, signature_fingerprint, redaction_policy
- Create `crates/exchange/src/redaction.rs` — RedactionEngine: load redaction policy (YAML/JSON specifying excluded categories, excluded file paths, excluded entities), apply redaction to case data before export, redaction audit log (what was redacted, why, when), redaction verification (confirm excluded categories absent from export)
- Create `crates/exchange/src/redaction/policy.rs` — policy schema: categories[] (browser_artifacts, email_content, personal_files, network_logs), entity_blacklist[], file_path_patterns[], min_date / max_date range

DTO:
- Add to `crates/transport/src/dto/exchange.rs`: ExportBundleDto, RedactionPolicyDto, RedactionAuditDto

Commands:
- Add to exchange_commands.rs: create_export_bundle, apply_redaction_policy, get_redaction_audit

Frontend:
- Create `frontend/src/app/pages/ExportWizard.tsx` — export wizard: step 1 (select format STIX/CASE/both), step 2 (configure redaction policy with category checkboxes, file path patterns), step 3 (signing: select key, preview signature), step 4 (download/save bundle)
- Add route `/export` in `frontend/src/app/routes.tsx`

Tests:
- `crates/exchange/tests/bundle_tests.rs` — bundle creation ZIP format, manifest completeness, signature embedded, bundle extraction verifies all files present
- `crates/exchange/tests/redaction_tests.rs` — browser artifacts excluded, specific file paths excluded, entity blacklist effective, redaction audit log accurate, verify excluded categories absent from export

**Phase 5: Inter-Tool Import (weeks 8-10)**

Autopsy XML import:
- Create `crates/exchange/src/import/autopsy.rs` — AutopsyXmlImporter: parse Autopsy XML report, extract file entries (name, path, size, timestamps, hash), extract artifacts (web history, recent documents, etc.), convert to FileEntry domain objects, insert into evidence graph
- Create `crates/exchange/src/import/autopsy_xml_parser.rs` — XML parsing with roxmltree or quick-xml: handle Autopsy 4.x report schema

Plaso CSV import:
- Create `crates/exchange/src/import/plaso.rs` — PlasoCsvImporter: parse Plaso timeline CSV (datetime, timestamp_desc, source, source_long, message, parser, display_name, etc.), convert to TimelineEvent domain objects, insert into timeline projection
- Create `crates/exchange/src/import/plaso_csv_parser.rs` — CSV parsing with csv crate: handle Plaso l2tcsv output format, timezone normalization to UTC

DTO:
- Add to `crates/transport/src/dto/exchange.rs`: ImportRequestDto (import_type: AutopsyXml/PlasoCsv, file_path, target_case_id), ImportResultDto (nodes_created, edges_created, events_imported, warnings[], errors[])

Commands:
- Add to exchange_commands.rs: import_autopsy_xml, import_plaso_csv, get_import_status

Tests:
- `crates/exchange/tests/import_autopsy_tests.rs` — valid Autopsy 4.x XML → FileEntry nodes with correct name/path/size/timestamps/hash, edge cases (empty report, missing optional fields, malformed XML error handling)
- `crates/exchange/tests/import_plaso_tests.rs` — valid Plaso CSV → TimelineEvent nodes with correct datetime/source/message, timezone normalization, duplicate event detection, edge cases (empty CSV, missing columns, invalid timestamps)

Fixture data:
- Create `testdata/fixtures/public-small/autopsy-sample.xml` — synthetic Autopsy report with 50 file entries + 10 web history records
- Create `testdata/fixtures/public-small/plaso-sample.csv` — synthetic Plaso timeline with 100 events across 5 sources
- Create matching expected JSON for assertion tests

**Phase 6: Production Hardening (weeks 10-11)**

Installer signing:
- Configure code signing certificate in `apps/desktop/src-tauri/tauri.conf.json` bundle.windows.wix signing
- Create `scripts/sign-installer.ps1` — sign MSI/EXE with Authenticode, timestamp server, verify signature post-sign
- Create `docs/v4-release-signing.md` — certificate acquisition, signing pipeline, verification steps

Auto-update:
- Configure Tauri updater plugin in `apps/desktop/src-tauri/src/lib.rs` — tauri_plugin_updater
- Configure update server endpoint in `apps/desktop/src-tauri/tauri.conf.json` plugins.updater
- Create `crates/infrastructure/src/updater.rs` — update check, download progress, signature verification on downloaded bundle
- Add update UI: `frontend/src/components/layout/UpdateBanner.tsx` — "Update available" banner with version info and changelog link

Crash reporting:
- Configure `apps/desktop/src-tauri/Cargo.toml` — add minidump crash handler dependency
- Create `crates/infrastructure/src/crash_handler.rs` — capture minidump on panic, write to `%APPDATA%/ForensicsWorkbench/crashes/`, crash report metadata (timestamp, version, stack trace if available)
- Create `scripts/upload-crash-report.ps1` — manual crash report submission (opt-in, no auto-upload)

Platform maturity:
- Run Windows App Certification Kit (WACK) on release build
- Test on Windows 10 (21H2+), Windows 11 (all editions), Windows Server 2022
- Create `docs/v4-platform-support-matrix.md` — tested OS versions, known limitations
- Configure CI release pipeline: build → sign → WACK → publish

Tests:
- `apps/desktop/src-tauri/tests/updater_tests.rs` — update check returns version, download verification
- `crates/infrastructure/tests/crash_handler_tests.rs` — minidump generated on simulated panic, metadata correct

**Phase 7: V4 Release Governance and RC Drill (weeks 11-12)**

- Create `docs/v4-release-checklist.md` — checklist: all stage AC met, all hard gates pass, guard scripts pass, V2+V3 regression suite green (1,345+228 tests), 5 FS reader trust framework verified, entity merge precision verified, STIX schema validation passes, custody log Merkle verification passes, installer signed and WACK passed
- Run full RC drill: `cargo test --workspace` (all Rust), `pnpm --dir frontend test` (all frontend), all guard scripts, benchmark regression check, import E01 sample + run V2 pipeline (real sample), export STIX + CASE + verify signatures, install signed MSI on clean Windows VM
- Compute V4 release scorecard: sum stage scores weighted by scoring table → final grade
- Create `docs/v4-release-notes.md` — release notes: new features summary, known issues, upgrade instructions
- Tag release commit: `git tag v4.0.0-rc1`, signed tag with GPG
- Update branch strategy: create `release/v4` branch from RC tag

#### Test Matrix

| # | Test Name | Scenario | Expected Result | Phase |
|---|-----------|----------|-----------------|-------|
| EX-01 | `test_stix_mapper_person_to_identity` | Person entity with email+display_name | Identity SCO with name, contact_information (email) | P1 |
| EX-02 | `test_stix_mapper_file_to_file_sco` | File entity with name, hashes, size | File SCO with name, hashes.MD5/SHA-256, size | P1 |
| EX-03 | `test_stix_mapper_relationship_to_sro` | CommunicatesWith relationship | Relationship SRO with relationship_type="communicates-with" | P1 |
| EX-04 | `test_stix_mapper_indicator_from_lead` | Correlation lead → Indicator | Indicator with pattern, pattern_type="stix", valid_from/to | P1 |
| EX-05 | `test_stix_serializer_bundle_format` | Export 3 SCOs + 2 SROs | valid STIX 2.1 bundle JSON, spec_version="2.1", type="bundle" | P1 |
| EX-06 | `test_stix_validator_required_fields` | STIX bundle missing a required field | validation error with specific field name | P1 |
| EX-07 | `test_stix_validator_reference_integrity` | SRO with non-existent source_ref | validation error, broken reference identified | P1 |
| EX-08 | `test_stix_end_to_end` | Case with leads+entities → STIX export | bundle validates against OASIS STIX 2.1 schema | P1 |
| EX-09 | `test_case_uco_file_mapping` | File entity with all facets | uco-observable:File with HashFacet, FileFacet, ContentDataFacet | P2 |
| EX-10 | `test_case_uco_identity_mapping` | Person entity | uco-identity:Identity with name, email | P2 |
| EX-11 | `test_case_serializer_jsonld` | UCO objects → JSON-LD | @context includes UCO namespaces, @type matches UCO class | P2 |
| EX-12 | `test_case_end_to_end` | Full case → CASE/UCO export | JSON-LD valid, UCO schema conformance | P2 |
| EX-13 | `test_signing_key_generation` | Generate Ed25519 keypair | keypair created, public key PEM valid, private key encrypted | P3 |
| EX-14 | `test_signing_sign_verify_roundtrip` | Sign bundle → verify with public key | verification passes | P3 |
| EX-15 | `test_signing_wrong_key_fails` | Sign with Key A, verify with Key B's public key | verification fails | P3 |
| EX-16 | `test_signing_tampered_bundle_fails` | Sign bundle, modify 1 byte, verify | verification fails | P3 |
| EX-17 | `test_custody_append_only` | Attempt to modify an existing custody entry | operation rejected or log integrity broken | P3 |
| EX-18 | `test_custody_merkle_consistency` | Append 10 events, check root after each | root changes with each append, recomputable from events | P3 |
| EX-19 | `test_custody_merkle_proof` | Request proof for entry 5 of 10 | proof verifies: entry 5 is in position 5 of the log | P3 |
| EX-20 | `test_custody_tamper_detection` | Modify entry 3, verify_log_integrity | tamper detected, root mismatch | P3 |
| EX-21 | `test_bundle_zip_creation` | Export STIX+CASE+manifest+signature → ZIP | valid ZIP, 4 files present, manifest accurate | P4 |
| EX-22 | `test_bundle_extraction_verification` | Extract bundle → verify signature → parse STIX | all steps succeed, data matches source case | P4 |
| EX-23 | `test_redaction_browser_excluded` | Redact browser artifacts from export | browser history/artifacts absent from export bundle | P4 |
| EX-24 | `test_redaction_specific_files_excluded` | Redact file paths matching pattern | files under redacted paths absent from export | P4 |
| EX-25 | `test_redaction_entity_blacklist` | Redact specific Person entity | entity and its relationships absent from export | P4 |
| EX-26 | `test_redaction_audit_log` | Apply redaction policy → check audit | audit lists every redacted item with category + reason | P4 |
| EX-27 | `test_redaction_verification` | Verify redacted export | confirm excluded categories absent from bundle contents | P4 |
| EX-28 | `test_import_autopsy_xml` | Valid Autopsy 4.x XML with 50 file entries | 50 FileEntry nodes created with correct name/path/size/timestamps/hash | P5 |
| EX-29 | `test_import_autopsy_xml_web_history` | Autopsy XML with 10 web history records | 10 web history artifacts created with correct URLs/timestamps | P5 |
| EX-30 | `test_import_autopsy_xml_invalid` | Malformed XML (missing closing tag) | error returned, no partial data imported | P5 |
| EX-31 | `test_import_plaso_csv` | Valid Plaso l2tcsv with 100 events | 100 TimelineEvent nodes with correct datetime/source/message | P5 |
| EX-32 | `test_import_plaso_csv_timezone` | Plaso CSV with non-UTC timestamps | all timestamps normalized to UTC | P5 |
| EX-33 | `test_import_plaso_csv_duplicates` | Plaso CSV with 5 duplicate events | duplicates detected and skipped, 95 unique events imported | P5 |
| EX-34 | `test_import_plaso_csv_invalid` | CSV with missing required columns | error with column name, no partial import | P5 |
| EX-35 | `test_updater_check_version` | Current 4.0.0, server has 4.0.1 | update notification shown, download available | P6 |
| EX-36 | `test_updater_signature_verify` | Download update, verify signature before install | valid signature → install; invalid → reject | P6 |
| EX-37 | `test_crash_handler_minidump` | Simulate panic → check crash directory | minidump file created, metadata JSON present | P6 |
| EX-38 | `test_installer_signing` | sign-installer.ps1 on release MSI | Authenticode signature present, timestamp valid, SmartScreen passes | P6 |
| EX-39 | `test_v4_rc_full_regression` | All Rust + frontend + guard scripts + E01 import + export roundtrip | scorecard >= 90 (A grade) | P7 |
| EX-40 | `test_v4_hard_gates_all` | All 8 hard gates in scoring section | zero failures | P7 |

#### Acceptance Criteria

- **STIX 2.1 export**: exported bundle validates against OASIS STIX 2.1 schema (stix2-validator or equivalent). At minimum: identity, file, ipv4-addr, network-traffic SCO types; relationship SRO with correct relationship_type; indicator SDO with pattern and pattern_type; observed-data SDO with valid Cyber Observable structure. All required fields per object type present.
- **CASE/UCO export**: exported JSON-LD conforms to CASE/UCO 1.3+ schema. @context includes UCO namespace declarations. Objects carry correct @type. Facets embedded per UCO specification. Reference integrity maintained across objects.
- **Digital signatures**: Ed25519 keypair generation produces valid PEM-encoded keys. Sign+verify roundtrip: verify(sign(bundle, sk), pk) = true. Wrong public key: verify(sign(bundle, sk_A), pk_B) = false. Tampered bundle (1 byte change): verify fails. Signature size < 200 bytes.
- **Chain-of-custody**: log is strictly append-only (no update/delete on custody_log rows). Merkle root recomputed correctly after each append. Merkle inclusion proof verifies for any entry position. Tampered entry detected via root mismatch. Proof generation < 100ms for up to 10,000 entries.
- **Case export bundle**: valid ZIP archive containing STIX JSON + CASE JSON-LD + manifest.json + signature.sig. Manifest accurately reflects export contents and redaction policy. Bundle can be extracted and signature verified independently of Forensics Workbench (standalone verification script provided).
- **Redaction**: browser artifacts, specific file paths, and entity blacklist categories all confirmed absent from redacted export. Redaction audit log records every exclusion with category and reason. Verification step confirms policy compliance.
- **Inter-tool import**: Autopsy 4.x XML produces correct FileEntry nodes (name, path, size, timestamps, hash all matching source). Plaso l2tcsv produces correct TimelineEvent nodes (datetime normalized to UTC, source, message preserved). Both handle invalid input gracefully (error message, no partial data).
- **Production hardening**: release MSI signed with Authenticode certificate. Windows SmartScreen check passes (no "unrecognized app" warning). Auto-update detects new version and verifies signature before install. Crash handler writes minidump to crashes directory with metadata.
- **V4 release scorecard**: >= 90 (A grade). All 8 hard gates pass. All guard scripts pass. V2 + V3 regression suites: 100% pass. Full RC drill: E01 import pipeline, STIX+CASE export roundtrip, signed installer install on clean VM.

#### Expected Results

**After Phase 1**: Running `cargo test -p exchange stix` shows all STIX mapping, serialization, and validation tests pass. The `export_stix` Tauri command takes a case ID and returns a valid STIX 2.1 JSON bundle. The export can be validated independently using the OASIS stix2-validator.

**After Phase 2**: Running `cargo test -p exchange case` shows CASE/UCO mapping and serialization tests pass. The `export_case` command with format=CASE produces valid JSON-LD. Both STIX and CASE exports can be generated from the same case.

**After Phase 3**: The `generate_signing_key` command creates an Ed25519 keypair stored encrypted in the case database. The `sign_export` command produces a detached signature. The chain-of-custody log is appended to automatically on evidence import/export operations. `verify_log_integrity` confirms the log has not been tampered with.

**After Phase 4**: The Export Wizard (`/export`) guides the user through format selection, redaction policy configuration (checkboxes for categories, text input for file patterns), and signing. The downloaded ZIP bundle contains STIX JSON, CASE JSON-LD, manifest, and signature files.

**After Phase 5**: Importing an Autopsy XML report creates FileEntry nodes visible in the file browser. Importing a Plaso CSV populates the timeline with events. Both imports show progress indicators and completion reports with node/event counts and any warnings.

**After Phase 6**: The Windows installer is Authenticode-signed and passes SmartScreen. The app shows an "Update Available" banner when a new version is detected. On crash, a minidump is written to `%APPDATA%/ForensicsWorkbench/crashes/` with metadata for manual submission.

**After Phase 7**: The V4 release scorecard is computed: all 8 hard gates pass, weighted stage scores sum to >= 90. The RC drill completes successfully: E01 import works, STIX+CASE export validates, signatures verify, the signed installer works on a clean VM. The git tag `v4.0.0-rc1` is signed and the `release/v4` branch is created.

---

## 4. Test Matrix

| # | 维度 | 场景 | 通过标准 | Stage |
|---|------|------|---------|-------|
| 1 | Entity Merge | Same person from email+username across artifacts | Merged entity with confidence > 0.8 |
| 2 | Entity Relationship | Communication pattern from email logs | CommunicatesWith edges created |
| 3 | Cross-Case Entity | Same email in 2 cases | Match found with confidence score |
| 4 | Entity Anomaly | Unusual login from foreign host | Anomaly flag raised |
| 5 | ext4 Parsing | public-small fixture | Expected JSON on guaranteed fields |
| 6 | XFS Parsing | public-small fixture | Expected JSON on guaranteed fields |
| 7 | Btrfs Parsing | public-small fixture | Expected JSON on guaranteed fields |
| 8 | APFS Parsing | public-small fixture | Expected JSON on guaranteed fields |
| 9 | HFS+ Parsing | public-small fixture | Expected JSON on guaranteed fields |
| 10 | Multi-OS Probe | GPT disk with ext4+APFS+NTFS | All 3 FS detected correctly |
| 11 | Deleted Recovery | ext4 journal replay | >= 80% recall |
| 12 | Deleted Recovery | APFS checkpoint rewind | >= 80% recall |
| 13 | Notebook Summary | 500-word entry → structured finding | Valid JSON with confidence indicator |
| 14 | Lead Narrative | 5-lead cluster → plain language | Cites specific artifact IDs |
| 15 | Evidence Gap | Ransomware template → missing artifacts | Correctly identifies gaps |
| 16 | NL Search | "find executables downloaded last week" | Correct structured query |
| 17 | Offline AI | AI inference without network | Zero network calls |
| 18 | STIX 2.1 Export | Correlation leads → STIX bundle | Schema validation passes |
| 19 | CASE/UCO Export | Full case → UCO model | Schema validation passes |
| 20 | Digital Signature | Export bundle signed | Ed25519 verification passes |
| 21 | Chain-of-Custody | Append events, verify Merkle | Proof verification passes |
| 22 | Case Redaction | Exclude browser artifacts from export | Redacted categories absent |
| 23 | Autopsy Import | Valid XML report → graph nodes | Expected node/edge counts |
| 24 | Plaso Import | Valid CSV timeline → graph nodes | Expected node/edge counts |
| 25 | Production Install | Signed installer on Windows | SmartScreen passes |
| 26 | V2 Regression | All V2 regression tests | All pass |
| 27 | V3 Regression | All V3 regression tests | All pass |
| 28 | V4 RC Drill | Full release candidate regression | Scorecard >= 90 |

---

## 5. Scoring Mechanism

| Stage | Weight | Focus |
|-------|--------|-------|
| V4-1: Entity Resolution | 35 | Entity merge, relationship inference, cross-case, anomaly |
| V4-2: Multi-OS Disk Images | 35 | ext4/XFS/Btrfs/APFS/HFS+ parsing + deleted recovery |
| V4-4: Exchange & Maturity | 30 | STIX/CASE export, signatures, custody, production |

### Hard Gates (any failure = Grade D)
- Entity merge integrity: merged entity does not link unrelated identities
- Multi-OS fixture regression: any public fixture fails on guaranteed fields
- AI offline guarantee: any network call from AI subsystem
- STIX schema validation: export fails OASIS validation
- Chain-of-custody integrity: tampered log detected as invalid
- V2 regression: any existing V2 test fails
- V3 regression: any existing V3 test fails
- Production signing: unsigned installer detected at RC

### Grade Interpretation
- **A (90-100)**: Ready for V4 release. All hard gates pass; all stages >=80%
- **B (80-89)**: Candidate release. All hard gates pass; >=3 stages at 80%+
- **C (70-79)**: Internal test only
- **D (<70)**: Do not release

---

## 6. Agent Division

- **Kepler** (Rust backend): ext4/XFS/Btrfs/APFS/HFS+ crates, entity resolution engine, AI inference integration, STIX/CASE exporters, chain-of-custody, signing infrastructure
- **Poincare** (Frontend): Entity Explorer, Entity Timeline, multi-OS partition viz, AI panel, summarization UI, narrative display, NL search bar, export/import wizards
- **Gauss** (Test & Data): ext4/XFS/Btrfs/APFS/HFS+ fixtures, multi-OS test disks, AI prompt test suite, STIX/CASE validation test cases, chain-of-custody tamper tests
- **Codex** (Integration & Release): IPC contract review, stage boundary enforcement, production signing pipeline, crash reporting setup, RC drill coordination, V4 release scorecard

### Execution order
```
V4-1 (Entity) ── starts first, independent
V4-2 (Disk Images) ── starts alongside V4-1 (no dependency)
V4-3 (AI) ── starts after V4-1 P2 (needs entity engine for narratives)
V4-4 (Exchange) ── starts after V4-2 P1 (needs disk image support for export)
```

---

## 7. V5 Directions (Preliminary)

1. **Real-Time Acquisition**: Live-response agent for Windows/Linux/macOS, memory image integration, streaming evidence
2. **Graph Query Language (GQL)**: Cypher-like DSL with autocomplete and query plan visualization
3. **Mobile & Cloud**: iOS/Android backup parsing, AWS/Azure/GCP cloud log ingestion
4. **Distributed Processing**: Multi-machine case processing with worker nodes (desktop-first, optional distributed)
5. **Investigation Marketplace**: Community rule pack sharing, investigation template exchange
