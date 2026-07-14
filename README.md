# Meow~Detective

A Tauri 2 desktop application for disk-image forensic analysis on Windows. The backend contains 36 Rust crates, 10 frontend pages, 98 Tauri commands, and approximately 2,061 Rust tests. Windows and Linux are the only production analysis platforms. macOS data-source requests and legacy macOS cases are unsupported; APFS/HFS+ may be identified as partition metadata, but no filesystem reader is instantiated. MIT licensed.

**V5 Quality Audit (2026-06):** Architecture compliance 97%, runtime safety 96%, forensic completeness 96%. E01 preview pipeline hardened with partition-indexed path reconstruction, MFT inode-based file resolution, and per-partition chunk-table caching.

## Architecture

```text
React UI (frontend/) -> Tauri commands / events
Tauri Command Layer (apps/desktop/src-tauri/) -> 98 commands
Application Services (crates/app-services/) -> 25 source modules
Core crates -> domain / evidence / persistence / search / timeline / artifacts / reports / MCP / graph
```

## Quick Start

### Frontend

```bash
cd frontend
pnpm install
pnpm dev
```

### Desktop

```bash
cd apps/desktop/src-tauri
cargo tauri dev
```

## Build

```bash
cd frontend && pnpm build
cd apps/desktop/src-tauri && cargo tauri build
```

## Test

```bash
cargo test --workspace
cd frontend && pnpm test            # Frontend (86 test files)
cd frontend && pnpm test:coverage
```

## Quality Gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd frontend && pnpm typecheck
cd frontend && pnpm lint
cd frontend && pnpm test
```

## Project Structure

| Directory | Description |
|---|---|
| `frontend/` | React 18 + TypeScript + Vite + Tailwind 4 |
| `apps/desktop/src-tauri/` | Tauri 2 shell (98 commands) |
| `crates/app-services/` | Application orchestration (25 source modules) |
| `crates/transport/` | Shared DTOs, commands, events, errors |
| `crates/persistence-sqlite/` | SQLite repos (21) and migration scripts (46) |
| `crates/evidence-core/` | Disk image probing and volume detection |
| `crates/fs-ntfs/`, `fs-fat/`, `fs-exfat/`, `fs-ext4/`, `fs-xfs/`, `fs-btrfs/` | Filesystem parsers (NTFS/FAT/ExFAT/ext4/XFS/Btrfs) |
| `crates/image-e01/`, `image-raw/` | Image readers (E01/RAW) |
| `crates/containers-pst/` | PST/OST/mbox email container parsers |
| `crates/exchange/` | Entity resolution and cross-case entity matching |
| `crates/fs-lvm/` | Linux LVM volume mapping and PV/LV offset translation |
| `crates/ceph-wire/`, `rocksdb-wire/` | Read-only Ceph BlueFS, RocksDB MANIFEST replay, and live-SST structure decoding |
| `crates/ingest/` | Ingestion pipeline orchestration |
| `crates/catalog/` | Catalog management and projections |
| `crates/artifacts-windows/` | Windows artifact parsers (Browser/EVTX/Prefetch/LNK/Registry[SYSTEM/SOFTWARE/NTUSER/SAM/txlog]/SRU/Thumbcache/JumpList) |
| `crates/artifacts-linux/` | Linux artifact parsers (journal/wtmp/bash/apt/cron/sudo) |
| `crates/search/` | Full-text indexing (tantivy) |
| `crates/timeline/` | Timeline generation |
| `crates/mcp-client/` | MCP client (SSE + Stdio) |
| `crates/reports/` | HTML / CSV / JSON reports |
| `crates/infrastructure/` | Shared utilities |

## Engineering Docs

- `docs/engineering-audit-plan.md`
- `docs/development-engineering-guide.md`
- `docs/design-constraints.md`
- `docs/model-architecture-algorithm-diagrams.md`
- `docs/documentation-index.md`
- `docs/progress-ledger.md`
- `docs/archive/README.md`
- `docs/v2-longterm-plan.md`
- `docs/validation-trust-framework.md`
- `docs/fixture-handbook.md`
- `docs/expected-json-contract.md`
- `docs/parser-support-matrix.md`
- `docs/known-unsupported-formats.md`
- `docs/error-taxonomy.md`
- `docs/error-classification-manual.md`
- `docs/benchmark-baseline.md`
- `docs/correlation-analysis-design.md`
- `docs/release-scorecard.md`
- `docs/mcp-security-model.md`
- `docs/export-and-media-safety.md`
- `docs/mcp-user-guide.md`
- `docs/real-sample-regression/README.md`
- `docs/benchmark-results/README.md`

## V2 Status

**V2 is ~90% complete.** Grade: **B (81/100)**. All 7 real E01 tests pass.

### What V2 delivered

- **V2-1 (Verifiable Trust): 95%** — public-small/medium fixture layers, expected JSON contracts per core chain (E01/NTFS/Prefetch/LNK/Registry/Recycle Bin), field guarantee levels (Guaranteed/Best-effort/Not-guaranteed), support matrix driven by sample verification, 7 real E01 regression cases all passing.
- **V2-2 (Cross-Artifact Correlation): 85%** — unified correlation model (node/edge/cluster/lead/confidence/provenance), 10+ rule families (LNK/Prefetch/Registry/RecycleBin/BrowserDownload/BrowserHistory/Email/JumpList), `CorrelationWorkspace` frontend, structured `Correlation Lead Details` in HTML/JSON/CSV reports, `familyCoverage[]` and `families[]` governance signals.
- **V2-3 (Performance & Scale): 70%** — benchmark harness and baseline datasets defined, cold/warm performance thresholds for medium/large cases, cancel/recovery for long-running tasks, p95 targets for search/timeline/file-tree. Remaining: automated nightly regression, memory/resource boundary enforcement in CI.
- **V2-4 (Security Governance & Release): 75%** — MCP permission model (resourceAccess/toolAccess/promptAccess/networkPolicy), export path safety (default overwrite=false), media handle short-lived lifecycle, error desensitization by taxonomy, release scorecard with hard gates, `/v2` governance dashboard with real-time signals from correlation, support matrix, error taxonomy, benchmark, and release policy.

### Governance fact sources

- `testdata/governance/v2-verification-catalog.json`
- `testdata/governance/v2-benchmark-baseline.json`
- `testdata/governance/v2-known-limitations.json`
- `testdata/governance/v2-release-policy.json`
- `testdata/governance/v2-runtime-results.json`
- `testdata/governance/v2-security-taxonomy.json`

## V3 Status

**V3 retained feature set is ~89% complete.** The current production platform slice retains the PST/OST/mbox and Linux artifact crates. The former macOS artifact slice was retired and is not a supported runtime entry.

### Retained crates

| Crate | Tests | Description |
|-------|-------|-------------|
| `crates/containers-pst/` | 63 | PST (Unicode 32/64), OST, mbox (RFC 4155) email parsing |
| `crates/artifacts-linux/` | 30 | systemd journal, wtmp, bash hist, apt/dpkg, cron, sudo |

### New capabilities

- **Evidence Graph**: 6 node types + 7 edge types, graph query API, /v3 dashboard
- **Browser parsers**: Chrome/Edge/Firefox history, downloads, cookies, sessions (123 tests)
- **Registry txlog**: .LOG1/.LOG2 transaction log parsing (18 tests)
- **Case Notebook**: CRUD + threading + evidence citations + step recording/replay
- **Rule Pack engine**: TOML-based declarative correlation rules with validation
- **Batch subsystem**: plan/build/monitor/resume/cancel for large cases
- **MBR full parsing**: EBR chain support for extended/logical partitions
- **Rayon parallelization**: CPU-bound artifact extraction + correlation matching

Current validation uses the repository quality gates listed above; historical phase test counts are not current workspace facts.

## V4 Status

**V4 retained core delivered.** Three Linux filesystem crates, entity resolution, STIX 2.1 export, Ed25519 signing, and chain-of-custody remain in production.

### New crates

| Crate | Description |
|---|---|
| `crates/fs-ext4/` | ext4 filesystem parser |
| `crates/fs-xfs/` | XFS filesystem parser |
| `crates/fs-btrfs/` | Btrfs filesystem parser |
| `crates/exchange/` | Entity resolution and cross-case entity matching |

### New capabilities

- **Filesystem parsers (ext4/XFS/Btrfs)**: read-only evidence readers for Linux filesystems
- **Entity resolution**: cross-case entity matching and canonicalization
- **STIX 2.1 export**: structured threat information sharing with STIX 2.1 JSON format
- **Ed25519 signing**: Ed25519 digital signature support for evidence integrity
- **Chain-of-custody**: custody tracking and audit trail for evidence handling

APFS/HFS+ partition-type recognition is metadata-only and remains `Unsupported`; it does not provide a file tree, preview, artifact extraction, or deleted-file recovery.

## License

MIT
