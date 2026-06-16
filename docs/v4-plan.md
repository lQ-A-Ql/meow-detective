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
1. Entity canonicalization and merge engine (weeks 1-3)
2. Entity relationship inference (weeks 3-6)
3. Cross-case entity matching (weeks 6-8)
4. Entity timeline and anomaly detection (weeks 8-10)
5. Frontend: Entity Explorer, Entity Timeline, Cross-Case Matches (weeks 10-12)
6. Documentation and governance (weeks 12-13)

#### Acceptance Criteria
- Person entities merged across email/username/display_name with confidence scores
- At least 3 relationship types inferred from graph edges
- Cross-case entity matching with configurable threshold
- Entity anomaly detection flags unusual access patterns

---

### Stage V4-2: Multi-OS Raw Disk Image Support（25 分，14 周）

#### Objective
Extend evidence ingestion from E01/NTFS to raw disk images containing Linux and macOS filesystems.

#### Stage boundaries
**In scope**: ext4 (full: inode tables, extent trees, directory hashing), XFS (v5: B+tree AGs, inode B+tree), Btrfs (basic: chunk tree, extent tree, subvolume listing), APFS (basic: container superblock, volume listing, checkpoint mapping), HFS+ (full: catalog B-tree, attributes, hard links), deleted file recovery (ext4 journal replay, APFS checkpoint rewind), multi-OS probe (detect partition types across OS boundaries in a single GPT disk).

**Deferred**: Btrfs RAID/compression/subvolume snapshots, APFS encryption/snapshot/clone fidelity, ZFS support.

#### Phase Tasks
1. ext4 reader crate + fixtures (weeks 1-3)
2. XFS reader crate + fixtures (weeks 3-5)
3. Btrfs reader crate + fixtures (weeks 5-7)
4. APFS reader crate + fixtures (weeks 7-9)
5. HFS+ reader crate + fixtures (weeks 9-10)
6. Multi-OS probe + deleted recovery (weeks 10-12)
7. Frontend: multi-OS partition visualization (weeks 12-13)
8. Integration tests + governance (weeks 13-14)

#### Acceptance Criteria
- ext4/XFS/Btrfs/APFS/HFS+ all parse public-small fixtures with expected JSON
- Deleted file recovery on ext4 and APFS at >= 80% recall on synthetic fixtures
- Multi-OS probe correctly identifies Windows+Linux+macOS partitions on a single GPT disk
- Graph contains platform discriminant on all File nodes

---

### Stage V4-3: AI-Assisted Investigation（25 分，12 周）

#### Objective
Add local, offline AI assistance for investigator workflows.

#### Stage boundaries
**In scope**: Notebook entry summarization (condense long entries into structured findings with confidence indicators), lead narrative generation (explain why a set of leads is interesting in plain language with evidence citations), evidence gap analysis (identify missing evidence types based on investigation templates), natural language search query assistance (convert "find all executables downloaded in the last week" to structured search).

**Deferred**: Full AI-driven investigation automation, evidence tampering detection, cloud-based AI integration.

#### Phase Tasks
1. Local LLM integration (llama.cpp or candle) — model loading, inference engine (weeks 1-3)
2. Notebook summarization pipeline (weeks 3-5)
3. Lead narrative generation (weeks 5-7)
4. Evidence gap analysis (weeks 7-8)
5. NL-to-structured-search query engine (weeks 8-10)
6. Frontend: AI panel, summarization UI, narrative display (weeks 10-11)
7. Privacy guard: all AI runs locally, no data leaves the machine (weeks 11-12)

#### Acceptance Criteria
- Notebook summarization produces valid structured findings from 500+ word entries
- Lead narrative generation cites specific evidence (artifact IDs, file paths)
- Evidence gap analysis correctly identifies missing artifact types per template
- NL search converts at least 5 query patterns to correct structured filters
- All AI inference runs offline; no network calls from AI subsystem

---

### Stage V4-4: Exchange, Chain-of-Custody & Platform Maturity（25 分，12 周）

#### Objective
Enable professional inter-tool exchange, cryptographically-verified chain-of-custody, and platform maturity for production deployment.

#### Stage boundaries
**In scope**: STIX 2.1 export (indicators, observed-data, relationships from correlation leads), CASE/UCO export (forensic case model), digital signatures on exports (Ed25519), chain-of-custody log with Merkle tree verification, case export bundle with selective redaction, inter-tool import (Autopsy XML report, Plaso timeline CSV), production hardening (installer signing, update mechanism, crash reporting).

#### Phase Tasks
1. STIX 2.1 export engine (weeks 1-3)
2. CASE/UCO export engine (weeks 3-5)
3. Digital signature + chain-of-custody (weeks 5-7)
4. Case export bundle with redaction (weeks 7-8)
5. Inter-tool import (weeks 8-10)
6. Production hardening: installer signing, auto-update, crash reporting (weeks 10-11)
7. V4 release governance + RC drill (weeks 11-12)

#### Acceptance Criteria
- STIX 2.1 export validates against OASIS schema
- Digital signatures verify on exported bundles using Ed25519 public key
- Chain-of-custody log is append-only and Merkle-verifiable
- Case export bundle can be redacted (exclude specified evidence categories)
- Autopsy XML and Plaso CSV import produce correct graph nodes
- Production installer passes Windows SmartScreen / code signing
- V4 release scorecard >= 90 (A grade)

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
| V4-1: Entity Resolution | 25 | Entity merge, relationship inference, cross-case, anomaly |
| V4-2: Multi-OS Disk Images | 25 | ext4/XFS/Btrfs/APFS/HFS+ parsing + deleted recovery |
| V4-3: AI Assistance | 25 | Summarization, narratives, gap analysis, NL search |
| V4-4: Exchange & Maturity | 25 | STIX/CASE export, signatures, custody, production |

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
