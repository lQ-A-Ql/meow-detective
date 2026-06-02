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
pnpm install

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

## Guard Scripts

These scripts run in CI and can also be run locally before a release branch.

```powershell
# Release/debug string guard
powershell -ExecutionPolicy Bypass -File scripts\check-release-guard.ps1

# Evidence media protocol safety guard. This verifies CSP/protocol/fallback
# wiring and rejects host-path asset URL regressions. It does not replace a
# manual Windows WebView2 video/audio seek smoke.
powershell -ExecutionPolicy Bypass -File scripts\check-media-protocol-guard.ps1

# Tauri command layer SQL boundary guard
powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1

# Dependency exception metadata/expiry guard
powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1
```

## Troubleshooting

### Frontend not loading
```bash
cd frontend
pnpm install
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
