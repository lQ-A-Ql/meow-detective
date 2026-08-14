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

## Parser Plugins

The standalone plugin workspace at `plugins-src/` is deliberately not part of
the repository workspace (see `docs/plugin-system-dev-test-plan.md`). Build it
and stage the DLLs into the exe-adjacent `plugins/<platform>/` layout the host
`plugin_loader` scans:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-plugins.ps1
```

The script compiles `plugins-src/` in release mode into `target/plugins-src/`,
then copies each plugin DLL to `target/release/plugins/<evidence-platform>/`
and, when present, to `apps/desktop/src-tauri/target/release/plugins/` next to
the `cargo tauri build` output. It is idempotent; the per-plugin platform
mapping inside the script must mirror each plugin's declared
`evidence_platform` in `meow_plugin_info`. Distribution stays green-software:
ship the DLL folder next to the exe in the zip; no NSIS bundle step.

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

# BitLocker credential boundary guard. This forbids Debug/Clone/Serialize on
# secret-bearing types, secret accessors reaching log/format/assert sinks in
# production code, plaintext volume materialization, and dropping
# #![forbid(unsafe_code)]. It also asserts the pinned upstream commit and the
# Apache-2.0 attribution files stay in place. Run with -SelfTest first.
powershell -ExecutionPolicy Bypass -File scripts\check-bitlocker-credential-guard.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-bitlocker-credential-guard.ps1

# Stage 0 architecture boundary guard. This prevents private fixture case IDs
# in frontend production code, requires media APIs to use COMMANDS.files, and
# restricts direct invoke usage to the API client. Backend Tauri/AppHandle
# findings are advisory by default and become fatal with -StrictBackend.
powershell -ExecutionPolicy Bypass -File scripts\check-stage0-boundary-guard.ps1

# Stage 2 platform boundary guard. This keeps transport platform DTOs out of
# app-services, requires symmetric Windows/Linux analyzers, prevents platform
# business logic from returning to analysis commands, locks ready-source
# aggregation and platform-scoped frontend views, and caps analysis facades.
powershell -ExecutionPolicy Bypass -File scripts\check-stage2-platform-boundary.ps1

# Stage 2 private real-sample gate. With no fixture arguments it reports an
# explicit skip; -RequireFixtures converts missing samples into a failure. When
# fixtures are present, both serial import orders are exercised by default.
powershell -ExecutionPolicy Bypass -File scripts\check-stage2-real-sample-isolation.ps1

# Stage 3 command/transport boundary guard. This caps compatibility facades,
# requires domain-split transport request modules, and prevents desktop command
# handlers from importing repositories, evidence readers, image readers, or
# filesystem parsers directly.
powershell -ExecutionPolicy Bypass -File scripts\check-stage3-command-boundary.ps1

# Stage 4 app-services boundary guard. This locks the complete decomposed
# capability wiring, Tauri-free dependency graph, serial evidence I/O modules,
# source-scoped file routing, bounded viewer ranges, sourceObjectId correlation,
# and non-fatal graph population. Run the adversarial self-test first.
powershell -ExecutionPolicy Bypass -File scripts\check-stage4-service-boundary.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-stage4-service-boundary.ps1

# Stage 5 parser/core boundary guard. This caps parser facades, preserves the
# established public parser entry points, and prevents Tauri runtime
# dependencies from entering parser and filesystem crates.
powershell -ExecutionPolicy Bypass -File scripts\check-stage5-parser-boundary.ps1

# Stage 6 physical test-separation guard. This requires a header-only test
# layout baseline, rejects test-only files under production src trees, and
# delegates bridge/body validation to the lexer-aware test-layout guard.
powershell -ExecutionPolicy Bypass -File scripts\check-stage6-test-separation.ps1

# Backend Rust module-size guard. This discovers every workspace member through
# cargo metadata, locks pre-existing production file-size debt against
# scripts/baselines/rust-module-size-baseline.csv, and fails on new or increased
# violations. Shared path/CSV/workspace policy lives in
# scripts/lib/RustGuard.Common.ps1.
# Cargo targets are scanned regardless of extension, but every production
# target must remain inside its owning package `src/`; only the exact root
# `build.rs` may sit outside it. Every `src/**/*.rs` file is scanned regardless
# of test-like names. Production `#[path] mod` and token-injecting `include!`
# are prohibited. Metadata has a 30-second default timeout
# (`RUST_GUARD_METADATA_TIMEOUT_MS`) with asynchronous output capture and
# exact-PID taskkill, kill-on-close Job Object, and bounded PID/parent/start-time
# Windows process-tree cleanup. Windows physical paths deduplicate
# case-insensitively; Linux paths remain case-sensitive.
powershell -ExecutionPolicy Bypass -File scripts\check-module-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-module-size.ps1

# New normal modules at 501-800 lines require a temporary, reviewed row in
# scripts/baselines/rust-module-size-exceptions.csv. Every row requires
# path/owner/reason/expires; stale, duplicate, expired, invalid, vendored, or
# migration-baseline-overlapping rows fail validation.
# Baseline edits are compared with RUST_MODULE_SIZE_BASELINE_REFERENCE in CI
# (or -ReferenceRevision locally) and may only reduce or delete existing debt.
# The first bootstrap additionally requires these protected repository values,
# supplied outside the pull request:
# RUST_MODULE_SIZE_BOOTSTRAP_SHA256=9b93323b627d11a62721b2c89bb3e366a3bc31f3cb12bf80b49e67b92698a8ec
# RUST_FUNCTION_SIZE_BOOTSTRAP_SHA256=78bc20765640ede2314f4d815acf03f29d461d21ad2ad9cca55db0e69c39770f
# RUST_TEST_LAYOUT_BOOTSTRAP_SHA256=9d5a89a0a26aaddc6ff822294cc262d4a1dec431ffa5b2b4d02194f552efba05

# Backend Rust function-size guard. A compiled comment/string-aware lexer scans
# the same non-vendored production roots, conservatively excludes items whose
# cfg expression provably implies test=true, and
# identifies functions by path/name/normalized-signature-hash/occurrence. The
# existing >100-line migration debt (including >150) may only shrink. Every
# non-baselined function >100 fails; 150 is the new-code hard ceiling. The
# in-memory synthetic fixture covers lexical and transition edge cases without
# leaving generated files in the repository. Function spans include attached
# attributes/modifiers; cfg exclusion is implication-based and conservative;
# const-generic brace expressions are balanced before angle-depth tracking.
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1

# Baseline edits compare against RUST_FUNCTION_SIZE_BASELINE_REFERENCE in CI,
# or -ReferenceRevision locally. Existing rows may only decrease or be deleted;
# added/changed identities, moved paths, and increased allowances fail. The
# one-time bootstrap manifest and protected RUST_FUNCTION_SIZE_BOOTSTRAP_SHA256
# variable must both pin the reference commit's initial baseline SHA-256.
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1 -ReferenceRevision origin/master

# Backend Rust test-layout guard. This locks tests embedded under src against
# scripts/baselines/rust-test-layout-baseline.csv and permits only external
# #[cfg(test)] + #[path = ".../tests/unit/..."] + mod tests; bridges after migration.
# The bridge module must be exactly non-public `tests`; its source-relative
# target must exist and canonically remain in the owning crate/app tests/unit
# folder without any reparse-point component, never a top-level integration
# test file. Known test attribute/macro aliases are counted. Baseline edits use
# RUST_TEST_LAYOUT_BASELINE_REFERENCE and may only reduce or delete debt.
# Explicit same-file `use ... as ...` alias chains are resolved to a fixed
# point. Cross-file wildcard/re-export graphs and unknown proc macros are not
# inferred and require a reviewed guard extension before use in `src` tests.
# Initial authorization also requires protected
# RUST_TEST_LAYOUT_BOOTSTRAP_SHA256 outside the pull request.
powershell -ExecutionPolicy Bypass -File scripts\check-rust-test-layout.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-rust-test-layout.ps1

# Private real-sample tests must be opt-in and cannot compile-include ignored testdata.
powershell -ExecutionPolicy Bypass -File scripts\check-private-real-sample-tests.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-private-real-sample-tests.ps1

# Documentation drift guard. This checks README/documentation-index
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
the median import phases, RSS peak, row count, throughput, NTFS shape, bounded
import-finalized Timeline projection, or system information parsing regress.
Keep thresholds machine-specific and pass the real sample through
`FORENSICS_E01_FIXTURE`; do not hard-code private paths in source.

```powershell
$env:FORENSICS_E01_FIXTURE = 'E:\path\to\sample.E01'
powershell -ExecutionPolicy Bypass -File scripts\check-e01-import-performance.ps1 -Runs 3
```

The default private `检材2.E01` gate expects at least 90,000 imported rows
against the stable 91,737-row baseline. Time, RSS, and throughput thresholds
remain independent hard gates; use `-MinRows` only when a different reviewed
fixture has a documented row baseline.

## Real PVE Cluster Import Gate

The PVE gate is opt-in and exercises the actual desktop background cluster
runner in strict serial mode. It verifies that all six members are attempted,
the three host `disk01` images produce isolated source databases and previewable
EXT4 trees, and the three Ceph BlueStore `disk02` images become metadata-only
sources rather than normal POSIX filesystems. Each BlueStore source retains an
isolated `source.db`, contains zero file entries, and persists a sanitized OSD
inventory with OSD IDs `0,1,2`, one shared cluster FSID, and unique OSD UUIDs.
It also requires one CRC-valid BlueFS superblock per OSD, unique BlueFS UUIDs,
sequence `50`, block size `4096`, shared-device layout, bounded transaction-log
replay, and exact RocksDB CURRENT/IDENTITY/active-MANIFEST control-plane
inventory. The live SST oracles are `35/40/33`; SST/WAL content and RADOS
objects are not reconstructed.

```powershell
$env:FORENSICS_PVE_CLUSTER_ROOT = 'E:\pangushi\服务器'
powershell -ExecutionPolicy Bypass -File scripts\check-pve-cluster-import.ps1 -RequireFixture
```

Without `-RequireFixture`, an unavailable private fixture is reported as an
explicit skip.

## Retained PVE RBD Preview Performance Gate

This opt-in gate reuses a retained case with a ready derived RBD source. It
does not import, enumerate, or materialize the VM tree. The gate builds the
three integration targets once, then runs the RBD workload, a native XFS
comparison from the Linux E01 fixture, and a PVE host `pve/root` EXT4 control
serially. It verifies fixed SHA-256 oracles, cross-run stability, viewer/media
byte parity, source/case invalidation, cold rebuild, and session convergence.
Cold file reads remain separate from the one-time three-OSD runtime
initialization. The RBD/native warm ratio uses a 1 ms denominator noise floor
for gating while preserving the raw ratio in the JSON report. Persisted logs
redact project, retained-case, comparison-fixture, and user-profile paths.

```powershell
$env:FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT = 'D:\path\to\retained-case'
$env:FORENSICS_LINUX_E01_FIXTURE = 'D:\path\to\native-linux.E01'
$env:FORENSICS_PVE_CLUSTER_ROOT = 'E:\path\to\pve-cluster'
powershell -ExecutionPolicy Bypass -File scripts\check-pve-rbd-preview-performance.ps1 `
  -RequireFixture -RequireComparisonFixtures -Runs 3
```

The workload covers direct XFS, `centos/home`, `centos/root`, files from
1,019 bytes through 614 MiB, repeated 64 KiB, sequential `16x64 KiB`,
sequential `4x1 MiB`, random large-file offsets, and the file tail. Missing
fixtures are an explicit skip unless `-RequireFixture` or
`-RequireComparisonFixtures` is supplied. Summaries and raw test output are written under
`artifacts/pve-rbd-preview-performance`.

For independent read-only RocksDB oracle inspection under WSL:

```bash
sudo bash scripts/dev/inspect-pve-rocksdb-manifest.sh \
  '/mnt/e/pangushi/服务器/server01/server01-disk02.E01'
```

The script exports BlueFS to a temporary directory, verifies the active
MANIFEST through `CURRENT`, prints `manifest_dump` control fields, and uses
`list_live_files_metadata` for the final live SST count. The
`manifestDumpSstRows` value is diagnostic only and must not be used as the live
set oracle.

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
