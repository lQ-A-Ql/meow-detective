# Forensics Workbench

A Tauri 2 desktop application for disk image forensic analysis on Windows. 22 Rust crates, 9 frontend pages, 73 Tauri commands. MIT licensed.

## Architecture

```text
React UI (frontend/) -> Tauri commands / events
Tauri Command Layer (apps/desktop/src-tauri/) -> 72 commands
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
| `apps/desktop/src-tauri/` | Tauri 2 shell (72 commands) |
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

## V2 Runtime

- `/v2` 页面已接入治理工作台与首版关联分析工作台
- 当前关联分析真实链路基于 shared `sourceObjectId` 聚合：
  - Artifact ↔ File
  - Timeline ↔ File
  - Artifact ↔ Timeline
- 治理事实源:
  - `testdata/governance/v2-verification-catalog.json`
  - `testdata/governance/v2-benchmark-baseline.json`
  - `testdata/governance/v2-known-limitations.json`
  - `testdata/governance/v2-release-policy.json`
  - `testdata/governance/v2-runtime-results.json`

## License

MIT
