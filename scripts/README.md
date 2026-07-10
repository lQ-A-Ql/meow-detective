# Forensics Workbench Scripts

## Build Scripts

### Windows
```batch
scripts\build-demo.bat
```

### Manual Build
```bash
# 1. Install frontend dependencies
cd frontend
pnpm install --frozen-lockfile

# 2. Build frontend
pnpm build

# 3. Build Tauri app
cd ..\apps\desktop\src-tauri
cargo build
```

## Running the Demo

### Development Mode (with hot reload)
```bash
cargo tauri dev
```

### Run Built Executable
```bash
target\debug\forensics-desktop.exe
```

### Production Build
```bash
cargo tauri build
```

## Demo Workflow

1. **Create a Case**
   - Click "New Case" on the home screen
   - Enter case name and examiner
   - Select a directory for the case

2. **Import Evidence**
   - Click "Import Data Source"
   - Select an evidence file (E01, RAW/DD, or directory)
   - Watch the import progress in the Jobs panel

3. **Browse Files**
   - Navigate the file tree on the left
   - Click files to view details
   - Use the hex/text viewer for file content

4. **Search**
   - Go to the Search page
   - Enter keywords to search across all indexed files
   - View highlighted results

5. **Timeline**
   - Go to the Timeline page
   - View file system events (MACB timestamps)
   - Filter by event type or time range

6. **Artifacts**
   - Go to the Artifacts page
   - View extracted Windows artifacts (Prefetch, LNK, etc.)
   - Filter by artifact type

7. **Reports**
   - Go to the Reports page
   - Generate HTML, JSON, or CSV reports
   - Export findings for documentation

## Coverage Reports

Coverage reports are generated explicitly. Frontend coverage currently enforces
an initial global baseline of 45% lines/statements/functions and 35% branches.
Backend coverage is reported as an LCOV artifact without a percentage threshold.

```powershell
# Frontend coverage, writes frontend/coverage
powershell -ExecutionPolicy Bypass -File scripts\run-coverage.ps1 -Frontend

# Rust coverage, requires cargo-llvm-cov and writes coverage/rust-lcov.info
cargo install cargo-llvm-cov --locked
powershell -ExecutionPolicy Bypass -File scripts\run-coverage.ps1 -Rust

# Both, skipping Rust coverage with a warning if cargo-llvm-cov is unavailable
powershell -ExecutionPolicy Bypass -File scripts\run-coverage.ps1
```

## Tiny Fixture Generation

The tiny fixture generator rewrites deterministic RAW, E01, and synthetic
Registry hives used by default CI. The registry hives are targeted Analysis
fixtures only; they are not production Windows hive samples.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\generate-tiny-fixtures.ps1
```

## Guard Scripts

These scripts run in CI and can also be run locally before a release branch.

```powershell
# Release/debug string guard
powershell -ExecutionPolicy Bypass -File scripts\check-release-guard.ps1

# Evidence media protocol safety guard. This verifies CSP/protocol/fallback
# wiring and rejects host-path asset URL regressions. It does not replace a
# manual Windows WebView2 video/audio seek smoke.
powershell -ExecutionPolicy Bypass -File scripts\check-media-protocol-guard.ps1

# WebView2 media smoke harness. This creates a temporary logical evidence
# directory with small inline and >20 MiB protocol media fixtures, then writes a
# checklist for manual playback/seek verification in the desktop WebView2 shell.
powershell -ExecutionPolicy Bypass -File scripts\run-webview2-media-smoke.ps1

# Tauri command layer SQL boundary guard
powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1

# Stage 0 architecture boundary guard. This prevents private fixture case IDs
# in frontend production code, requires media APIs to use COMMANDS.files, and
# restricts direct invoke usage to the API client. Backend Tauri/AppHandle
# findings are advisory by default and become fatal with -StrictBackend.
powershell -ExecutionPolicy Bypass -File scripts\check-stage0-boundary-guard.ps1

# Documentation drift guard. This checks README/AGENTS/documentation-index
# factual counts, required engineering-doc entries, and Mermaid block count.
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1

# Documentation archive guard. This validates strict UTF-8 decoding, the
# type/month archive taxonomy, manifest counts, routing targets, and prevents
# historical audit/remediation files from returning to the docs root.
powershell -ExecutionPolicy Bypass -File scripts\check-doc-archive.ps1

# Optional full Mermaid render validation. Requires Chrome/Edge or an available
# Puppeteer browser; renders all diagrams to temporary SVG files.
powershell -ExecutionPolicy Bypass -File scripts\check-doc-drift.ps1 -RenderMermaid

# Dependency exception metadata/expiry guard
powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1

# EVTX dependency decision guard. This prevents reintroducing the legacy
# evtx -> encoding dependency path after the local encoding_rs patch.
powershell -ExecutionPolicy Bypass -File scripts\check-evtx-dependency-decision.ps1

# Import optimization guard. This locks the E01/RAW beta-closeout decisions:
# real samples use FORENSICS_E01_FIXTURE, Timeline stays deferred for image
# imports, staging-only PRAGMAs stay aggressive, and app.db stays conservative.
powershell -ExecutionPolicy Bypass -File scripts\check-import-optimization-guard.ps1
```

## Real E01 Import Profiling

Real E01 profiling is opt-in because the fixture is private and multi-GB. The
runner executes the ignored desktop import regression test repeatedly, parses
`[import-profile]` phase lines, and writes Markdown/JSON summaries under
`artifacts/import-profiles`.

```powershell
$env:FORENSICS_E01_FIXTURE = 'E:\path\to\sample.E01'
powershell -ExecutionPolicy Bypass -File scripts\run-e01-import-profile.ps1 -Runs 3
```

## Real E01 Import Performance Gate

The performance gate is also opt-in. It runs the profile harness, then fails if
the median import phases, RSS peak, row count, throughput, NTFS shape, lazy
Timeline projection, or system information parsing regress. Keep thresholds
machine-specific and pass the real sample through `FORENSICS_E01_FIXTURE`; do
not hard-code private paths in source.

```powershell
$env:FORENSICS_E01_FIXTURE = 'E:\path\to\sample.E01'
powershell -ExecutionPolicy Bypass -File scripts\check-e01-import-performance.ps1 -Runs 3
```

## Troubleshooting

### Frontend not loading
```bash
cd frontend
pnpm install --frozen-lockfile
pnpm build
```

### Build errors
```bash
# Clean and rebuild
cargo clean
cargo build
```

### Missing dependencies

- **Rust**: Install from https://rustup.rs/
- **Node.js**: Install from https://nodejs.org/
- **pnpm**: Run `npm install -g pnpm`
