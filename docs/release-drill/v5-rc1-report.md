# Release Candidate Drill Report

## v5.0.0-rc1 — 2026-06-20

---

## Regression Results

### Fixture Regression

| Suite    | Total | Passed | Failed | Status |
|----------|-------|--------|--------|--------|
| Rust     | ____  | ____   | ____   | ____   |
| Frontend | ____  | ____   | ____   | ____   |

### Security Regression

| Guard Script                         | Status |
|--------------------------------------|--------|
| check-command-sql-boundary.ps1       | ____   |
| check-media-protocol-guard.ps1       | ____   |
| check-release-guard.ps1              | ____   |
| check-stage5-regression-guard.ps1    | ____   |
| check-frontend-lockfile-policy.ps1   | ____   |
| check-deny-exceptions.ps1            | ____   |
| check-evtx-dependency-decision.ps1   | ____   |
| check-import-optimization-guard.ps1  | ____   |
| check-doc-drift.ps1                  | ____   |
| check-benchmark-regression.ps1       | ____   |

- **cargo-deny**: ____ (no unapproved licenses, no advisories).

### Performance

| Benchmark        | Time | Notes          |
|------------------|------|----------------|
| liuyang E01      | ____ |                |
| jc2 E01          | ____ |                |
| large-tier RAW   | ____ | (if applicable)|

### Rule Pack

- **v5-builtin.toml** built-in rule pack validated: ____ (all rules parse, no dangling references).

### Graph Integrity

- Node and edge counts verified against expected values on the medium fixture: ____

---

## V5 Production Deployment Gates

| Gate                          | Status   | Detail                                      |
|-------------------------------|----------|---------------------------------------------|
| **Updater**                   |          |                                             |
| update-check                  | ____     | `updater::check_for_update` returns manifest|
| update-download               | ____     | `updater::download_update` verifies SHA-256 |
| update-apply                  | ____     | `updater::apply_update` launches installer  |
| manifest-endpoint-reachable   | ____     | Remote manifest endpoint responds correctly |
| **Crash Handler**             |          |                                             |
| panic-hook-installed          | ____     | `crash_handler::set_panic_hook` set at init |
| crash-report-written          | ____     | JSON report lands in `crash_reports/`       |
| path-sanitization             | ____     | No case data or absolute paths in report    |
| system-diagnostics            | ____     | OS, arch, CPU count captured in report      |
| **Marketplace**               |          |                                             |
| pack-list-fetched             | ____     | `MarketplaceBrowser` renders available packs|
| pack-download                 | ____     | Download progress bar and status banner work|
| pack-import                   | ____     | Imported rules are available for extraction |
| pack-rating                   | ____     | Star rating UI submits and persists rating  |
| mock-data-fallback            | ____     | Mock packs render without Tauri backend     |
| **Release Quality**           |          |                                             |
| code-signing                  | ____     | Windows binary signed with valid certificate|
| installer-smoke               | ____     | MSI installs and launches on clean VM       |
| self-update-smoke             | ____     | Update from v4.x to v5.0.0-rc1 succeeds     |
| crash-recovery                | ____     | App restarts cleanly after simulated crash  |
| telemetry-opt-out             | ____     | No telemetry enabled by default (GDPR safe) |
| **Documentation**             |          |                                             |
| changelog                     | ____     | `CHANGELOG.md` lists all V5 features        |
| upgrade-guide                 | ____     | `docs/upgrade-v4-to-v5.md` exists           |
| known-issues                  | ____     | `docs/known-issues.md` is up to date        |

---

## Release Scorecard

| Category          | Score   | Max | Notes                                    |
|-------------------|---------|-----|------------------------------------------|
| Verification      | ____    | 25  | Fixture regression + graph integrity     |
| Correlation       | ____    | 25  | Cross-case entity matching walkthroughs  |
| Performance       | ____    | 20  | Benchmark thresholds met                 |
| Security          | ____    | 25  | All guard scripts + cargo-deny clean     |
| Production Readiness| ____  | 5   | Updater, crash handler, marketplace      |
| **Total**         | **____**| 100 | **Grade ____**                           |

---

## Residual Risks

1. **Remote update endpoint not yet deployed** — The updater crate compiles and tests pass, but the production update server hosting `UpdateManifest` JSON has not been provisioned. Until the endpoint is live, `check_for_update` will return a fetch error (handled gracefully: the app continues without blocking).
2. **Crash report upload path unspecified** — Crash reports are written to local `crash_reports/` directory. An opt-in upload mechanism (e.g. to a Sentry-compatible endpoint) is not implemented in this RC.
3. **Marketplace backend not integrated** — The `MarketplaceBrowser` component uses mock data in non-Tauri mode. Tauri command handlers for `list_marketplace_rule_packs`, `download_marketplace_rule_pack`, and `import_rule_pack` are not yet wired.
4. **Self-update not smoke-tested** — The update flow (`check → download → apply`) compiles but has not been exercised end-to-end against a real installer due to the missing endpoint (see risk 1).
5. **Large-tier benchmarks missing** — Performance envelope at scale (images > 500 GB) is unvalidated. Low probability of regression given medium results, but worth closing before GA.

---

## Rollback Procedure

1. Stop all running ingest jobs and close the active case.
2. Close the desktop application.
3. If the v5.0.0-rc1 was installed via MSI, use **Apps & Features** to uninstall it.
4. Restore the previous stable release binary (`v4.0.0`).
5. Re-open cases with the restored binary; verify case integrity via `CaseService::validate_integrity`.
6. Verify that the `crash_reports/` directory (if any reports were generated) is preserved for debugging.
