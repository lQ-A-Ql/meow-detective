# Forensics Workbench V5 Execution Plan

> **状态：历史设计记录，不是当前范围。** 本文档保留 2026-06 的阶段设计原样，
> 便于追溯决策，但其中的基线数字与四根柱子已被代码否决。当前事实以
> `docs/documentation-index.md` §4.3c、`docs/progress-ledger.md` 和
> `docs/parser-support-matrix.md` 为准。
>
> 已失效之处：
>
> - 基线数字（31 crates、1,483 Rust + 228 frontend tests）已过期；当前为 28 crates、~3,038 个 Rust 测试函数、86 个前端测试文件
> - 8 个文件系统 reader 的说法包含 APFS/HFS+，与 Stage 1 平台边界冲突；生产 reader 为 NTFS/FAT/exFAT/ext4/XFS/Btrfs 六个，APFS/HFS+ 只做分区类型元数据识别
> - **Stage V5-1（高级文件系统取证）：部分落地** — NTFS/ext4/XFS 已删除文件恢复与 header/footer carving 已实现；journal replay、APFS checkpoint rewind、Btrfs snapshot diff、`$LogFile`/USN/ADS 未实现
> - **Stage V5-2（移动与云）：已退役** — `artifacts-ios`、`artifacts-android`、`cloud-audit` crate 于 `a3c1f265` 移除；对应 transport DTO 亦已删除，仅 `dto/android.rs` 保留为预留契约面
> - **Stage V5-3（GQL）：已退役** — `gql` crate 与未挂载的前端 `features/gql` 空壳均已移除
> - **Stage V5-4（生产化与市场）：已退役** — `updater`、`crash_handler` crate 与 `features/marketplace` 空壳均已移除
>
> V5 期间实际累积的工程量在 Ceph/PVE 集群重建（`ceph-wire`、`rocksdb-wire`）与
> Windows registry / 浏览器凭据深度，两者都不在本文档的原始规划中。

## 0. V5 Baseline (as of 2026-06-17)

V5 builds on V4 core: 31 crates, 1,483 Rust + 228 frontend tests, 8 filesystem readers (NTFS/FAT/ExFAT/ext4/XFS/Btrfs/APFS/HFS+), entity resolution, STIX 2.1 export, Ed25519 signing, Merkle custody.

### Constraints
- Windows-primary, desktop-first, single-user, disk-forensic focused
- No memory images, no PCAP, no live agent — pure disk forensics
- All new parsers enter V2-1 trust framework (fixture + expected JSON + guarantee levels)
- **iOS/Android 取证逻辑独立 crate**：禁止混入 app-services/artifacts-windows/artifacts-core
- **云审计日志独立 crate**：禁止混入 app-services 或 exchange

---

## 1. V5 Goals

Deepen disk-forensic capabilities with **advanced filesystem recovery, mobile/cloud disk artifacts, a graph query language, and production deployment**.

### Five pillars

1. **Advanced Filesystem Forensics** — ext4/XFS journal replay, APFS checkpoint rewind, Btrfs snapshot diff, NTFS ADS/$LogFile/USN, file carving, FileVault detection.

2. **Mobile & Embedded Disk Forensics** — iOS/Android backup parsing from disk images, embedded storage forensics, firmware extraction.

3. **Cloud Log Disk Forensics** — AWS CloudTrail/Azure/GCP/M365 audit logs ingested as file evidence (no network API).

4. **Graph Query Language (GQL)** — Cypher-like DSL with autocomplete, saved queries, query plan visualization.

5. **Production Deployment** — Installer signing, auto-update, crash reporting, rule pack marketplace.

---

## 2. Key Interfaces & DTOs

### 2.1 Filesystem Recovery

| DTO | Purpose |
|-----|---------|
| `DeletedFileRecoveryDto` | Recovered file: original path, method, confidence |
| `JournalReplayResultDto` | ext4/XFS replay: recovered inodes, orphaned blocks |
| `SnapshotDiffDto` | Btrfs snapshot comparison: added/removed/changed |
| `ADSMetadataDto` | NTFS alternate data stream names, sizes |

### 2.2 Mobile & Cloud

| DTO | Purpose |
|-----|---------|
| `IosArtifactDto` | Contact, Message, Photo, SafariHistory, CallLog |
| `AndroidArtifactDto` | SMS, Contact, ChromeHistory, AppData |
| `CloudAuditEntryDto` | Normalized: source, action, principal, target, timestamp |

### 2.3 GQL

| DTO | Purpose |
|-----|---------|
| `GqlQueryDto` | Query string + parameters |
| `GqlQueryResultDto` | Nodes, edges, aggregates, query plan |
| `SavedQueryDto` | Named query with parameters |

### 2.4 Production

| DTO | Purpose |
|-----|---------|
| `UpdateManifestDto` | Version, release notes, download URL |
| `CrashReportDto` | Sanitized crash report (no case data) |
| `MarketplacePackDto` | Rule pack: name, version, author, rating |

---

## 3. Stage Design

### Stage V5-1: Advanced Filesystem Forensics（30 分，15 周）

#### Objective
Add deleted file recovery, snapshot analysis, and deep filesystem analysis across all 8 supported filesystems.

#### Phase Tasks

**Phase 1: ext4/XFS Journal Replay (weeks 1-4)**
- ext4: parse journal superblock, descriptor blocks, commit blocks
- Recover deleted inodes from journal transactions
- XFS: parse log format, recover metadata operations
- Test: >=80% recall on synthetic 20-file deletion fixture

**Phase 2: APFS Checkpoint + Btrfs Snapshots (weeks 4-7)**
- APFS: parse checkpoint descriptor, rewind to prior state
- Recover files deleted since last checkpoint
- Btrfs: list snapshots, compute file-level diff between snapshots
- Test: APFS deleted recovery on synthetic fixture

**Phase 3: NTFS Deep Analysis (weeks 7-10)**
- $LogFile parsing: record undo/redo operations
- USN Journal parsing: file change history
- Alternate Data Stream extraction
- Test: ADS names + content match expected

**Phase 4: File Carving (weeks 10-12)**
- Header/footer-based carving: JPEG, ZIP, PDF, Office
- Carve from unallocated clusters in NTFS/ext4
- Test: >=80% precision on synthetic mixed-content image

**Phase 5-6: Frontend + Governance (weeks 12-15)**
- Deleted file browser with recovery method badges
- Snapshot diff viewer (side-by-side)
- ADS panel in file inspector
- Documentation

#### Acceptance Criteria
- ext4 journal replay: >=80% recall on 20-file fixture
- APFS rewind: recover files deleted within last checkpoint
- NTFS ADS: extract stream names + content
- File carving: recover JPEG + ZIP headers
- Btrfs snapshot diff: list added/removed/changed files
- All features produce DTOs with confidence scores

---

### Stage V5-2: Mobile & Cloud Disk Forensics（25 分，14 周）

#### Objective
Add iOS/Android backup parsing from disk images and cloud audit log ingestion as file-based evidence.

#### Architecture constraint
iOS 和 Android 取证核心逻辑**独立存放**，各自新建专属 crate，不杂糅进 `app-services` 或 `artifacts-windows`。遵循现有 artifacts-linux/artifacts-macos 的分 crate 模式：

```
crates/artifacts-ios/       ← iOS 独立 crate (备份解析、plist、SQLite)
crates/artifacts-android/   ← Android 独立 crate (ADB 解析、SMS/MMS)
crates/cloud-audit/         ← 云审计日志独立 crate (CloudTrail/Azure/GCP/M365)
```

禁止将 iOS/Android parser 放入 `app-services`、`artifacts-windows`、`artifacts-core`。

#### Phase Tasks

**Phase 1: crates/artifacts-ios (weeks 1-4)**
- `crates/artifacts-ios/Cargo.toml` — workspace deps only
- `crates/artifacts-ios/src/lib.rs` — module declarations
- `crates/artifacts-ios/src/backup.rs` — Manifest.db parser, file listing
- `crates/artifacts-ios/src/contacts.rs` — AddressBook.sqlitedb parser
- `crates/artifacts-ios/src/messages.rs` — sms.db parser
- `crates/artifacts-ios/src/photos.rs` — Photos.sqlite parser
- `crates/artifacts-ios/src/safari.rs` — Safari History.db parser
- `crates/artifacts-ios/src/calls.rs` — CallHistory.storedata parser
- `crates/artifacts-ios/src/notes.rs` — Notes.sqlite parser
- Transport DTOs: `crates/transport/src/dto/ios.rs`

**Phase 2: crates/artifacts-android (weeks 4-7)**
- `crates/artifacts-android/Cargo.toml`
- `crates/artifacts-android/src/lib.rs`
- `crates/artifacts-android/src/backup.rs` — ADB .ab format, tar decompress
- `crates/artifacts-android/src/contacts.rs` — contacts2.db parser
- `crates/artifacts-android/src/sms.rs` — mmssms.db parser
- `crates/artifacts-android/src/chrome.rs` — Chrome History (similar to Windows)
- `crates/artifacts-android/src/calls.rs` — calllog.db parser
- Transport DTOs: `crates/transport/src/dto/android.rs`

**Phase 3: crates/cloud-audit (weeks 7-10)**
- `crates/cloud-audit/Cargo.toml`
- `crates/cloud-audit/src/lib.rs`
- `crates/cloud-audit/src/aws.rs` — CloudTrail JSON parser
- `crates/cloud-audit/src/azure.rs` — Azure Activity Log JSON parser
- `crates/cloud-audit/src/gcp.rs` — GCP Audit Log JSON parser
- `crates/cloud-audit/src/m365.rs` — M365 Unified Audit Log CSV parser
- `crates/cloud-audit/src/normalize.rs` — unified CloudAuditEntry
- Transport DTOs: `crates/transport/src/dto/cloud_audit.rs`

**Phase 4-5: Integration + Frontend (weeks 10-14)**
- Ingest integration: detect backup/audit files, route to parser
- Multi-source timeline: merge local + cloud events
- Frontend: Mobile device view, cloud timeline panel
- Documentation + fixtures

#### Acceptance Criteria
- `crates/artifacts-ios/`: 5+ artifact types, public-small fixtures, expected JSON
- `crates/artifacts-android/`: 4+ artifact types, public-small fixtures, expected JSON
- `crates/cloud-audit/`: 4 cloud providers, normalized entries
- All 3 crates are **fully independent** — zero coupling to app-services or each other
- Multi-source timeline: local + cloud events in single view

---

### Stage V5-3: Graph Query Language（25 分，12 周）

#### Objective
Deliver a Cypher-like GQL for querying the Evidence Graph.

#### Phase Tasks
1. GQL parser + execution engine (weeks 1-4)
2. Query plan + optimization (weeks 4-6)
3. Frontend: editor, autocomplete, syntax highlighting (weeks 6-8)
4. Saved queries + parameter templates (weeks 8-10)
5. Query plan visualization (weeks 10-11)
6. Documentation (weeks 11-12)

#### Acceptance Criteria
- MATCH, WHERE, RETURN queries execute on Evidence Graph
- Autocomplete suggests node types, edge types, predicates
- Saved queries with parameter substitution
- Query plan shows traversal cost estimates

---

### Stage V5-4: Production Deployment & Marketplace（20 分，10 周）

#### Objective
Production-grade deployment and community ecosystem.

#### Phase Tasks
1. Installer signing (Windows Authenticode) (weeks 1-2)
2. Auto-update via Tauri updater (weeks 2-4)
3. Crash reporting (local-first, sanitized) (weeks 4-6)
4. Rule pack marketplace (browse, download, rate) (weeks 6-8)
5. Production RC drill + scorecard >=90 (weeks 8-9)
6. V5 release packaging (weeks 9-10)

#### Acceptance Criteria
- Installer passes Windows SmartScreen
- Auto-update downloads + applies new version
- Crash reports sanitized (no case data leaked)
- Rule pack marketplace: browse, search, download, rate
- V5 release scorecard >= 90 (A grade)

---

## 4. Test Matrix

| # | 场景 | 通过标准 | Stage |
|---|------|---------|-------|
| 1 | ext4 journal replay | >=80% recall, 20-file fixture | V5-1 |
| 2 | XFS log replay | Deleted inodes recovered | V5-1 |
| 3 | APFS checkpoint rewind | Files from last checkpoint recovered | V5-1 |
| 4 | Btrfs snapshot diff | File diff between 2 snapshots | V5-1 |
| 5 | NTFS ADS extraction | Stream names + content | V5-1 |
| 6 | NTFS $LogFile parsing | Operations with timestamps | V5-1 |
| 7 | USN Journal parsing | File change history | V5-1 |
| 8 | File carving (JPEG) | Recover JPEG from unallocated | V5-1 |
| 9 | File carving (ZIP) | Recover ZIP from unallocated | V5-1 |
| 10 | iOS Contacts | Public-small fixture, expected JSON | V5-2 |
| 11 | iOS Messages | Public-small fixture, expected JSON | V5-2 |
| 12 | iOS Photos | Public-small fixture, expected JSON | V5-2 |
| 13 | iOS Safari | Public-small fixture, expected JSON | V5-2 |
| 14 | Android SMS | Public-small fixture, expected JSON | V5-2 |
| 15 | Android Contacts | Public-small fixture, expected JSON | V5-2 |
| 16 | Android Chrome | Public-small fixture, expected JSON | V5-2 |
| 17 | CloudTrail ingestion | Normalized entries from JSON file | V5-2 |
| 18 | Azure Audit ingestion | Normalized entries from JSON file | V5-2 |
| 19 | GCP Audit ingestion | Normalized entries from JSON file | V5-2 |
| 20 | M365 Audit ingestion | Normalized entries from CSV file | V5-2 |
| 21 | Multi-source timeline | Local + cloud merged | V5-2 |
| 22 | GQL MATCH | Correct nodes/edges | V5-3 |
| 23 | GQL WHERE | Confidence filter works | V5-3 |
| 24 | GQL RETURN | Aggregate functions | V5-3 |
| 25 | GQL autocomplete | Suggests valid types | V5-3 |
| 26 | Saved queries | Parameter substitution | V5-3 |
| 27 | Installer signing | SmartScreen passes | V5-4 |
| 28 | Auto-update | Download + apply | V5-4 |
| 29 | Crash reporting | Sanitized, no case data | V5-4 |
| 30 | Rule pack marketplace | Browse/download/rate | V5-4 |
| 31 | V2/V3/V4 regression | All existing tests pass | V5-4 |
| 32 | V5 RC drill | Scorecard >= 90 | V5-4 |

---

## 5. Scoring

| Stage | Weight | Focus |
|-------|--------|-------|
| V5-1: Filesystem Forensics | 30 | Journal replay, snapshots, ADS, carving |
| V5-2: Mobile & Cloud | 25 | iOS/Android, cloud audit logs |
| V5-3: GQL | 25 | Parser, engine, frontend, saved queries |
| V5-4: Production | 20 | Signing, update, crash, marketplace |

### Hard Gates
- Deleted recovery precision: false positive rate < 10%
- Evidence immutability: disk analysis never modifies source
- GQL query isolation: cannot modify graph data
- Privacy: crash reports contain no case data
- Production signing: unsigned installer = gate failure
- V2/V3/V4 regression: all pass
- Documentation drift: support matrix current

### Grade
- A (90-100): Release | B (80-89): Candidate | C (70-79): Internal | D (<70): Blocked

---

## 6. V6 Directions

1. **Real-Time Acquisition** — Live-response agent for running systems (deferred from V4)
2. **Memory Forensics** — Memory dump integration with Volatility-style analysis
3. **Network Forensics** — PCAP ingestion and flow analysis
4. **AI-Assisted Investigation** — Local LLM for summarization and gap analysis
5. **Distributed Processing** — Multi-machine worker nodes
