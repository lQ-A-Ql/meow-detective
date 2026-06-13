# Forensics Workbench

A Tauri 2 desktop application for disk image forensic analysis on Windows. 22 Rust crates, 9 frontend pages, 73 Tauri commands. MIT licensed.

## Architecture

```text
React UI (frontend/) -> Tauri commands / events
Tauri Command Layer (apps/desktop/src-tauri/) -> 73 commands
Application Services (crates/app-services/) -> 18 source modules
Core crates -> domain / evidence / persistence / search / timeline / artifacts / reports / MCP
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
cd frontend && pnpm test            # Frontend (42 test files)
cd frontend && pnpm test
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
| `apps/desktop/src-tauri/` | Tauri 2 shell (73 commands) |
| `crates/app-services/` | Application orchestration (18 source modules) |
| `crates/transport/` | Shared DTOs, commands, events, errors |
| `crates/persistence-sqlite/` | SQLite repos (9) and migration scripts (23) |
| `crates/evidence-core/` | Disk image probing and volume detection |
| `crates/fs-ntfs/`, `fs-fat/`, `fs-exfat/` | Filesystem parsers |
| `crates/image-e01/`, `image-raw/` | Image readers |
| `crates/search/` | Full-text indexing |
| `crates/timeline/` | Timeline generation |
| `crates/artifacts-windows/` | Windows artifact parsers |
| `crates/mcp-client/` | MCP client |
| `crates/reports/` | HTML / CSV / JSON reports |
| `crates/infrastructure/` | Shared utilities |

## Engineering Docs

- `docs/engineering-audit-plan.md`
- `docs/development-engineering-guide.md`
- `docs/design-constraints.md`
- `docs/model-architecture-algorithm-diagrams.md`
- `docs/documentation-index.md`
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

## V3 Planning

V3 will build on the V2 productized foundation with these directions (see `docs/v2-longterm-plan.md` Section 8):

- **Evidence Graph** — unify files, artifacts, timeline, entities, leads into a queryable graph model.
- **Broader coverage** — PST/OST/mbox email, Registry transaction logs, more browser versions, more filesystems, Linux/macOS artifacts.
- **Reproducible investigation narratives** — case notebook, evidence citations, analysis step replay, report-operation history linkage.
- **Rule packs & templates** — investigation templates, hit rule bundles, org-level verification configs, interpretability scoring strategies.
- **Offline batch & multi-stage orchestration** — recoverable, queuable, phased local batch execution for very large cases, while keeping desktop-first.

## License

MIT
