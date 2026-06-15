# Release Candidate Drill Report

## v3.0.0-rc1 — 2026-06-15

---

## Regression Results

### Fixture Regression

| Suite   | Total | Passed | Failed | Status |
|---------|-------|--------|--------|--------|
| Rust    | 1301  | 1301   | 0      | PASS   |
| Frontend| 228   | 228    | 0      | PASS   |

### Security Regression

| Guard Script                         | Status |
|--------------------------------------|--------|
| check-command-sql-boundary.ps1       | PASS   |
| check-media-protocol-guard.ps1       | PASS   |
| check-release-guard.ps1              | PASS   |
| check-stage5-regression-guard.ps1    | PASS   |
| check-frontend-lockfile-policy.ps1   | PASS   |
| check-deny-exceptions.ps1            | PASS   |

- **cargo-deny**: clean (no unapproved licenses, no advisories).

### Performance

| Benchmark        | Time | Notes          |
|------------------|------|----------------|
| liuyang E01      | 168s | 7 tests run    |
| jc2 E01          | 70s  | passed         |

### Rule Pack

- **v2-standard.toml** built-in rule pack validated: all rules parse, no dangling references.

### Graph Integrity

- Node and edge counts verified against expected values on the medium fixture.

---

## Release Scorecard

| Category       | Score   | Max | Notes                              |
|----------------|---------|-----|------------------------------------|
| Verification   | 22      | 25  | Medium fixture suite still missing |
| Correlation    | 21      | 25  | 3 walkthroughs completed           |
| Performance    | 18      | 20  | Large-tier benchmarks needed       |
| Security       | 23      | 25  | Audit log coverage is partial      |
| **Total**      | **84**  | 100 | **Grade B**                        |

---

## Quality Gates

| Gate                       | Status   | Detail                              |
|----------------------------|----------|-------------------------------------|
| core-fixture-regression    | PASS     | 1301 Rust + 228 frontend all green  |
| docs-drift                 | PASS     | No stale doc references detected    |
| benchmark-thresholds       | WARNING  | Large-tier benchmarks incomplete    |
| security-baseline          | PASS     | All 6 guard scripts pass, deny clean|
| evidence-hash              | WARNING  | E01 hash verification not fully automated |
| runtime-failures           | PASS     | No unexpected panics or crashes     |
| correlation-family-coverage| PASS     | All required families covered       |

---

## Residual Risks

1. **Large-tier benchmarks missing** — Performance envelope at scale (images > 500 GB) is unvalidated. Low probability of regression given medium results, but worth closing before GA.
2. **Evidence hash automation gap** — E01 hash verification relies on manual steps in the current toolchain. An automated pre-ingest hash check command is planned for v3.1.
3. **Audit log partial coverage** — Only create/delete case events are logged; file access and export operations are not yet traced. Medium severity, no PII at risk.
4. **Medium fixture suite incomplete** — Larger synthetic fixtures (multi-volume RAW, 50 GB E01) are not yet in CI. Test coverage focused on small/tiny fixtures.

---

## Rollback Procedure

1. Stop all running ingest jobs and close the active case.
2. Close the desktop application.
3. Delete the `v3.0.0-rc1` binary/directory from the deployment target.
4. Restore the previous stable release binary (`v2.8.0`).
5. Re-open cases with the restored binary; verify case integrity via `CaseService::validate_integrity`.
