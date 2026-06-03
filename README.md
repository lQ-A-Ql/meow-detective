# Forensics Workbench

A Tauri 2 desktop application for disk image forensic analysis on Windows. 22 Rust crates, 8 frontend pages, 52 Tauri commands. MIT licensed.

## Architecture

```
React UI (frontend/)  —  Vite + Tailwind 4 + React 18 + Zustand + TanStack Query
  ↓ Tauri invoke / events
Tauri Command Layer (apps/desktop/src-tauri/commands/)  —  52 commands
  ↓
Application Services (crates/app-services/)  —  19 service modules
  ↓
Core Crates: domain, evidence-core, fs-ntfs, fs-fat, fs-exfat, image-e01, image-raw,
             search, timeline, artifacts-windows, catalog, ingest, mcp-client,
             reports, persistence-sqlite, infrastructure
```

## Quick Start

### Prerequisites

- Rust stable (see `rust-toolchain.toml`)
- Node.js 20+ with pnpm
- Windows 10/11 (primary platform)

### Frontend (mock mode)

```bash
cd frontend
pnpm install
pnpm dev
```

### Full Desktop App

```bash
cd apps/desktop/src-tauri
cargo tauri dev
```

## Build

```bash
# Frontend production build (~600KB, gzip ~170KB)
cd frontend && pnpm build

# Desktop release bundle (~23MB executable)
cd apps/desktop/src-tauri && cargo tauri build
```

## Test

```bash
# Backend (541+ tests)
cargo test --workspace

# Frontend (81 tests, 24 files)
cd frontend && pnpm test

# Frontend with coverage
cd frontend && pnpm test:coverage
```

## Quality Gates

```bash
cargo fmt --all -- --check          # Rust formatting
cargo clippy --workspace --all-targets -- -D warnings  # Lint
cargo test --workspace              # 541+ backend tests
cd frontend && pnpm typecheck       # TypeScript strict check
cd frontend && pnpm lint            # ESLint
cd frontend && pnpm test            # 81 frontend tests (Vitest)
```

## Project Structure

| Directory | Description |
|-----------|-------------|
| `frontend/` | React 18 + TypeScript 5 + Vite 6 + Tailwind 4 |
| `apps/desktop/src-tauri/` | Tauri 2 shell (52 commands) |
| `crates/domain/` | Core entities (Case, FileEntry, Artifact, etc.) |
| `crates/transport/` | Shared DTOs, commands, events, errors |
| `crates/app-services/` | Application-layer orchestration (19 services) |
| `crates/persistence-sqlite/` | SQLite repos (9) and migrations (18) |
| `crates/evidence-core/` | Disk image probing, volume detection |
| `crates/fs-ntfs/`, `fs-fat/`, `fs-exfat/` | Filesystem parsers |
| `crates/image-e01/`, `image-raw/` | Image format readers |
| `crates/search/` | Full-text indexing (tantivy) |
| `crates/timeline/` | Timeline event generation |
| `crates/artifacts-windows/` | Windows artifact parsers (EVTX, Prefetch, LNK, Registry, etc.) |
| `crates/catalog/` | File catalog indexing and projections |
| `crates/ingest/` | Ingestion pipeline orchestration |
| `crates/mcp-client/` | MCP client (SSE + Stdio transports) |
| `crates/reports/` | Report generation (HTML, CSV, JSON) |
| `crates/infrastructure/` | Cross-cutting utilities |

## License

MIT
