# Forensics Workbench V5 Execution Plan

## 0. V5 Baseline (as of 2026-06-17)

V5 builds on V4 core delivered: 31 crates, 1,483 Rust + 228 frontend tests, 8 filesystem readers (NTFS/FAT/ExFAT/ext4/XFS/Btrfs/APFS/HFS+), entity resolution, STIX 2.1 export, Ed25519 signing, Merkle custody. Grade: A- (9.0/10).

### Architectural constraints carried forward

- Windows-primary, desktop-first, single-user
- No HTTP server; Tauri commands/events only
- `crates/transport` is the sole IPC contract source
- Evidence is read-only
- SQL stays in `persistence-sqlite` or lower
- All new parsers enter V2-1 trust framework
- UTF-8 for all docs/fixtures/benchmarks

---

## 1. V5 Goals

V5 elevates Forensics Workbench from a "professional investigation platform" to a **complete incident response workstation** with **real-time acquisition, advanced filesystem forensics, mobile/cloud intelligence, and production-grade deployment**.

### Five pillars

1. **Real-Time Evidence Acquisition** — Live-response agent for Windows/Linux/macOS, memory image integration, PCAP ingestion, streaming evidence pipeline (start analysis while acquisition is in progress).

2. **Advanced Filesystem Forensics** — Deleted file recovery for Linux (ext4 journal replay, XFS log replay), macOS (APFS checkpoint rewind), Btrfs snapshots, APFS encryption detection, MFT-resident data carving, alternate data stream deep analysis.

3. **Mobile & Cloud Artifacts** — iOS backup/image parsing (Contacts, Messages, Photos, Safari), Android backup parsing (SMS/MMS, Chrome, app data), cloud audit log ingestion (AWS CloudTrail, Azure Audit, GCP Audit, M365 Unified Audit Log).

4. **Graph Query Language (GQL)** — Cypher-like DSL for querying the Evidence Graph with path patterns, filters, and aggregations. Autocomplete, syntax highlighting, saved queries, query plan visualization.

5. **Production Deployment & Marketplace** — Installer signing, auto-update, crash reporting, telemetry (opt-in), community rule pack marketplace, investigation template sharing, production RC drill with full scorecard >= 90.

---

## 2. Key Interfaces & DTOs

### 2.1 Real-Time Acquisition DTOs

| DTO | Purpose |
|-----|---------|
| `LiveAgentConfigDto` | Collection profile: target paths, artifact families, exclusion rules |
| `LiveAgentSessionDto` | Session state: connected, collection progress, errors |
| `StreamingEvidenceDto` | Partial evidence chunk with progress metadata |
| `MemoryImageDto` | Memory dump metadata: process list, kernel modules, network connections |
| `PCAPSessionDto` | Network capture session: packets, flows, protocols |

### 2.2 Advanced Filesystem DTOs

| DTO | Purpose |
|-----|---------|
| `DeletedFileRecoveryDto` | Recovered file metadata: original path, recovery method, confidence |
| `JournalReplayResultDto` | ext4/XFS journal replay: recovered inodes, orphaned blocks |
| `SnapshotListingDto` | Btrfs subvolume snapshots with creation time and parent |
| `ADSMetadataDto` | NTFS alternate data stream names, sizes, types |

### 2.3 Mobile & Cloud DTOs

| DTO | Purpose |
|-----|---------|
| `IosArtifactDto` | Union: Contact, Message, Photo, SafariHistory, CallLog, Note |
| `AndroidArtifactDto` | Union: SMS, Contact, ChromeHistory, AppData, CallLog |
| `CloudAuditEntryDto` | Normalized cloud audit log: source, action, principal, target, timestamp |
| `MultiCloudTimelineDto` | Merged timeline from local + cloud evidence |

### 2.4 GQL DTOs

| DTO | Purpose |
|-----|---------|
| `GqlQueryDto` | Query string + parameters |
| `GqlQueryResultDto` | Nodes, edges, aggregates, query plan |
| `GqlSchemaDto` | Available node types, edge types, predicates |
| `SavedQueryDto` | Named query with parameters and description |

### 2.5 Production DTOs

| DTO | Purpose |
|-----|---------|
| `UpdateManifestDto` | Latest version, release notes, download URL |
| `CrashReportDto` | Stack trace, system info, case state (sanitized) |
| `TelemetryEventDto` | Anonymous usage event: feature, duration, result |
| `MarketplacePackDto` | Rule pack metadata: name, version, author, rating, downloads |

---

## 3. Stage Design

### Stage V5-1: Real-Time Evidence Acquisition（25 分，14 周）

#### Objective
Enable live-response collection from running systems and streaming evidence ingestion.

#### Stage boundaries
**In scope**: Live-response agent for Windows (PowerShell-based collector), Linux (SSH-based collector), macOS (bash-based collector). Memory image integration: load memory dump alongside disk image in same case. PCAP ingestion: read pcap/pcapng files, extract flows and sessions. Streaming evidence pipeline: begin file tree/artifact analysis while acquisition is still running.

**Deferred**: Kernel-level live response, remote agent orchestration, real-time network monitoring.

#### Phase Tasks
1. Live agent framework + Windows collector (weeks 1-3)
2. Linux + macOS live collectors (weeks 3-5)
3. Memory image integration (weeks 5-7)
4. PCAP ingestion (weeks 7-9)
5. Streaming evidence pipeline (weeks 9-11)
6. Frontend: Live Acquisition panel, streaming progress, memory integration (weeks 11-13)
7. Documentation + governance (weeks 13-14)

#### Acceptance Criteria
- Windows collector extracts: registry hives, EVTX, Prefetch, LNK, browser history
- Linux collector extracts: systemd journal, wtmp, bash history, auth logs
- Memory image loads alongside disk image; graph links process-to-file
- PCAP ingestion produces flow records with IP/port/protocol
- Streaming pipeline: FileBrowser available while acquisition still running

---

### Stage V5-2: Advanced Filesystem Forensics（25 分，15 周）

#### Objective
Add deleted file recovery, snapshot analysis, and deep NTFS analysis.

#### Stage boundaries
**In scope**: ext4 journal replay (recover deleted inodes from journal), XFS log replay, APFS checkpoint rewind, Btrfs snapshot listing and diff, NTFS alternate data stream extraction, NTFS $LogFile/USN journal parsing, file carving from unallocated space, APFS encryption detection (FileVault).

**Deferred**: Full encryption decryption, RAID reconstruction, distributed filesystem analysis.

#### Phase Tasks
1. ext4 journal replay engine (weeks 1-3)
2. XFS log replay engine (weeks 3-5)
3. APFS checkpoint rewind engine (weeks 5-7)
4. Btrfs snapshot diff (weeks 7-8)
5. NTFS ADS + $LogFile/USN parsing (weeks 8-10)
6. File carving from unallocated space (weeks 10-12)
7. Frontend: Deleted file browser, snapshot diff viewer, ADS panel (weeks 12-14)
8. Documentation + governance (weeks 14-15)

#### Acceptance Criteria
- ext4 journal replay: >= 80% recall on synthetic 20-file fixture
- APFS checkpoint rewind: recover files deleted within last checkpoint
- NTFS ADS extraction: list and read alternate data streams
- File carving: recover at least JPEG and ZIP headers from unallocated space
- All new recovery features produce DeletedFileRecoveryDto with confidence scores

---

### Stage V5-3: Mobile & Cloud Artifacts（25 分，14 周）

#### Objective
Add iOS, Android, and cloud audit log evidence sources.

#### Stage boundaries
**In scope**: iOS backup parsing (iTunes/Finder backup: Manifest.db, Contacts, Messages, Photos, Safari History, Call Log, Notes). Android backup parsing (ADB backup: contacts, SMS/MMS, Chrome history, call log). Cloud audit logs: AWS CloudTrail, Azure Activity Log, GCP Audit Logs, Microsoft 365 Unified Audit Log.

**Deferred**: Encrypted backups, physical extraction, social media artifacts, cloud storage content.

#### Phase Tasks
1. iOS backup parser crate (weeks 1-4)
2. Android backup parser crate (weeks 4-7)
3. Cloud audit log parsers (AWS/GCP/Azure) (weeks 7-10)
4. M365 Unified Audit Log parser (weeks 10-11)
5. Multi-cloud timeline integration (weeks 11-12)
6. Frontend: Mobile device view, cloud timeline (weeks 12-13)
7. Documentation + governance (weeks 13-14)

#### Acceptance Criteria
- iOS backup: Contacts, Messages, Photos, Safari History parse with expected JSON
- Android backup: SMS, Contacts, Chrome History parse with expected JSON
- CloudTrail: normalized entries with principal/action/target
- Multi-cloud timeline: merged local + cloud events in single view
- All new parsers enter trust framework with public-small fixtures

---

### Stage V5-4: Graph Query Language & Production Deployment（25 分，13 周）

#### Objective
Deliver a production-ready product with Graph Query Language, installer signing, auto-update, and community marketplace.

#### Stage boundaries
**In scope**: GQL parser and execution engine (Cypher-inspired: MATCH, WHERE, RETURN). Autocomplete + syntax highlighting in frontend. Saved queries with parameter templates. Query plan visualization. Installer signing (Windows Authenticode). Auto-update via Tauri updater. Crash reporting (sentry-like, local-first). Anonymous telemetry (opt-in). Community rule pack marketplace (browse, download, rate). Investigation template sharing. Full production RC drill with scorecard >= 90.

**Deferred**: GQL federation (multi-case), online marketplace hosting, cloud sync.

#### Phase Tasks
1. GQL parser + execution engine (weeks 1-3)
2. GQL frontend: editor, autocomplete, query plan (weeks 3-5)
3. Saved queries + templates (weeks 5-6)
4. Installer signing + auto-update (weeks 6-8)
5. Crash reporting + telemetry (weeks 8-9)
6. Rule pack marketplace (weeks 9-11)
7. Production RC drill + scorecard (weeks 11-12)
8. V5 release packaging (weeks 12-13)

#### Acceptance Criteria
- GQL: MATCH, WHERE, RETURN queries execute on Evidence Graph
- Saved queries stored per-case with parameter templates
- Installer passes Windows SmartScreen with Authenticode signature
- Auto-update downloads and applies new version
- Crash reports sanitized (no case data leaked)
- Rule pack marketplace: browse, search, download, rate
- V5 release scorecard >= 90 (A grade)

---

## 4. Test Matrix

| # | 维度 | 场景 | 通过标准 | Stage |
|---|------|------|---------|-------|
| 1 | Windows Live Agent | Collect from test VM | Registry + EVTX + Prefetch extracted |
| 2 | Linux Live Agent | Collect from test VM | journal + wtmp + bash extracted |
| 3 | Memory Integration | Load .dmp alongside .E01 | Process-to-file graph edges created |
| 4 | PCAP Ingestion | Load test pcap | Flow records with IP/port/protocol |
| 5 | Streaming Pipeline | Partial acquisition + browse | FileBrowser shows files before acquisition complete |
| 6 | ext4 Journal Replay | Synthetic 20-file fixture | >= 80% recall |
| 7 | XFS Log Replay | Synthetic fixture | Deleted inodes recovered |
| 8 | APFS Checkpoint | Synthetic fixture | Files from last checkpoint recovered |
| 9 | Btrfs Snapshot Diff | 2 snapshots | File diff between snapshots |
| 10 | NTFS ADS | File with ADS | Stream names and content extracted |
| 11 | NTFS $LogFile | Synthetic fixture | Operations parsed |
| 12 | File Carving | Unallocated space with JPEG header | JPEG recovered |
| 13 | iOS Backup | Public-small fixture | Contacts + Messages + Photos parsed |
| 14 | Android Backup | Public-small fixture | SMS + Contacts + Chrome parsed |
| 15 | AWS CloudTrail | Test log file | Normalized entries |
| 16 | Azure Audit | Test log file | Normalized entries |
| 17 | GCP Audit | Test log file | Normalized entries |
| 18 | M365 Audit | Test log file | Normalized entries |
| 19 | Multi-Cloud Timeline | Local + cloud merge | Single timeline view |
| 20 | GQL MATCH | MATCH (f:File)-[e:Contains]->(c) | Correct nodes/edges returned |
| 21 | GQL WHERE | Filter by confidence > 0.7 | Correct filtering |
| 22 | GQL RETURN | Aggregate count(*) | Correct count |
| 23 | Saved Queries | Save + load + execute | Parameter substitution works |
| 24 | Installer Signing | Windows Authenticode | SmartScreen passes |
| 25 | Auto-Update | New version available | Downloads + applies update |
| 26 | Crash Reporting | Simulated crash | Report captured, no case data leaked |
| 27 | Telemetry | Opt-in usage event | Anonymous event recorded |
| 28 | Marketplace | Browse + download pack | Pack loads and validates |
| 29 | V2 Regression | All V2 tests | All pass |
| 30 | V3 Regression | All V3 tests | All pass |
| 31 | V4 Regression | All V4 tests | All pass |
| 32 | V5 RC Drill | Full regression | Scorecard >= 90 |

---

## 5. Scoring Mechanism

| Stage | Weight | Focus |
|-------|--------|-------|
| V5-1: Real-Time Acquisition | 25 | Live agents, memory, PCAP, streaming |
| V5-2: Advanced Filesystem Forensics | 25 | Deleted recovery, snapshots, ADS, carving |
| V5-3: Mobile & Cloud | 25 | iOS/Android, cloud audit logs |
| V5-4: GQL & Production | 25 | GQL, signing, update, marketplace |

### Hard Gates (any failure = Grade D)
- Live agent data integrity: collected evidence matches source system
- Deleted recovery precision: false positive rate < 10% on synthetic fixtures
- Evidence immutability: acquisition does not modify source evidence
- GQL query isolation: queries cannot modify graph data
- Privacy: crash reports contain no case data
- Production signing: unsigned installer at RC = gate failure
- V2/V3/V4 regression: any existing test fails
- Documentation drift: support matrix >1 release out of date

### Grade Interpretation
- **A (90-100)**: Ready for V5 release
- **B (80-89)**: Candidate release
- **C (70-79)**: Internal test only
- **D (<70)**: Do not release

---

## 6. Agent Division

- **Kepler** (Rust backend): Live agent framework, memory/PCAP integration, journal replay engines, file carving, iOS/Android parsers, cloud audit parsers, GQL engine, signing/update infrastructure
- **Poincare** (Frontend): Live acquisition panel, streaming progress, deleted file browser, snapshot diff viewer, ADS panel, mobile device view, cloud timeline, GQL editor, marketplace UI, update UX
- **Gauss** (Test & Data): Live agent test VMs, synthetic journal/snapshot/carving fixtures, iOS/Android backups, cloud audit logs, GQL test suite, marketplace test packs, signing test certs
- **Codex** (Integration & Release): Agent-to-platform contract review, evidence immutability audit, production signing pipeline, crash report privacy audit, RC drill coordination, V5 release scorecard

### Execution order
```
V5-1 (Acquisition) — starts first, independent
V5-2 (Filesystem) — starts alongside V5-1 (no dependency)
V5-3 (Mobile/Cloud) — starts after V5-1 P1 (needs live agent patterns)
V5-4 (GQL/Production) — starts after V5-2 P1 (needs filesystem metadata for GQL queries)
```

---

## 7. V6 Directions (Preliminary)

1. **Distributed Case Processing** — Worker nodes for multi-machine import/analysis, desktop-first with optional distributed workers
2. **AI-Assisted Investigation** (revisited) — Local LLM integration with improved on-device inference models
3. **Advanced Threat Intelligence** — STIX/TAXII feed ingestion, threat actor profiling, campaign correlation
4. **Investigation Collaboration** — Multi-user case review via peer-to-peer sync, review comments, approval workflows
5. **Blockchain Chain-of-Custody** — Immutable custody log with distributed verification
