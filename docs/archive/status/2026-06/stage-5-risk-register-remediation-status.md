# Stage 5 Risk Register and Remediation Status

> Archived: 2026-06 remediation status snapshot. It is not the current risk register.

**Date**: 2026-06-09  
**Scope**: Stage 5 documentation checkpoint for the high/medium remediation items identified in `docs/full-security-audit-2026-05-29.md`, `docs/full-functional-audit-2026-05-29.md`, and the later project audit in `docs/full-project-audit-2026-06-01.md`.  
**Baseline document**: `docs/remediation-plan-v2.1.md`.

## Status legend

| Status | Meaning |
|---|---|
| Completed | Remediation is implemented and has at least targeted verification recorded in `docs/remediation-plan-v2.1.md`. |
| Completed / guarded | Remediation is implemented and a CI or repository guard exists to prevent regression. |
| Completed / targeted | Remediation is implemented for the product path promised by v2.1, but broader parser/product coverage remains explicitly out of scope. |
| Partial | A safe baseline is implemented, but the full cleanup or production-grade validation remains open. |
| Blocked / manual | Code or harness exists, but closure requires a manual fixture, environment, or upstream dependency action. |

## Stage mapping

| Stage | Remediation theme | High/Medium items tied to this stage | Current stage status |
|---|---|---|---|
| Stage 1 | Security hardening at command, parser, report, CSP, pagination, viewer, migration, LIKE, and recent-case boundaries | H-01, H-02, H-03, H-04, H-05, M-02, M-04, M-06, M-07, M-08, M-09 | Completed; relevant items are either directly verified or guarded by targeted tests and scripts. |
| Stage 2 | Real analysis/provenance and evidence-reader consistency | Project audit P0/P1 analysis risks; supports H-02/M-07 by keeping reads on validated evidence paths | Completed / targeted; Registry remains targeted, not a full hive browser. |
| Stage 3 | Preview, extraction, navigation, saved queries, settings, and media safety | F-02, F-03, F-04, F-05-adjacent import UX, media preview risk from project audit | Completed / guarded, except real Windows WebView2 seek evidence remains manual. |
| Stage 4 | Events, job semantics, reports provenance, and IPC/event contract | F-01, F-05, F-06, report trust risks related to H-04/M-09 | Completed; event contract and report escaping/provenance have regression coverage. |
| Stage 5 | CI, dependency gates, SBOM, fixtures, coverage, release guards | M-10, M-12, parser/fixture verification blockers for Stage 2/3, dependency residual risks | Completed baseline; residual risks are explicitly tracked below. |

## Security H/M risk register

| ID | Original severity | Original finding | Remediation stage | Current status | Verification evidence | Residual risk / blocker |
|---|---:|---|---|---|---|---|
| H-01 | High | `create_case` accepted path traversal in the case name. | Stage 1 | Completed | `docs/remediation-plan-v2.1.md` Phase 1.1 and Phase 0.1 record case-name/path validation regression coverage. | None known beyond maintaining validation tests when case creation UI/API changes. |
| H-02 | High | `import_data_source` accepted unrestricted source paths. | Stage 1, Stage 2 | Completed | Phase 1.1 records import request validation for empty path, NUL, Windows device path, extended-length path, reserved device names, existence, and file/directory type; Stage 2 evidence reads use `FileEntryId + DataSourceKind`. | Product still intentionally allows user-selected local evidence import in a single-user desktop app; threat model relies on local operator consent and dialog/validation boundaries rather than a global evidence-directory sandbox. |
| H-03 | High | NTFS `read_file_data` could allocate unbounded memory from malicious data runs. | Stage 1 | Completed | Phase 1.3 records data-run/buffer limit checks; final gate includes workspace tests and filesystem targeted tests. | Continue fuzzing/fixture expansion for malformed NTFS edge cases; no current release blocker recorded. |
| H-04 | High | HTML report generation could emit unescaped dynamic content. | Stage 1, Stage 4 | Completed | Phase 4.6 records HTML escaping regression and report provenance; final gate includes `cargo test -p reports`. | None known; keep report exporters in regression scope when adding new fields. |
| H-05 | High | Tauri app lacked a strict CSP. | Stage 1, Stage 3, Stage 5 | Completed / guarded | Phase 3.1 records media CSP boundaries; `scripts/check-media-protocol-guard.ps1` and release guard are in CI evidence. | CSP now intentionally permits `media-src 'self' data: evidence-media:` for preview; any new protocol or external source must update the guard deliberately. |
| M-01 | Medium | Unsafe lifetime transmute in the MFT reader cancellation path. | Stage 1 | Completed | Later development log entries record replacing borrowed cancellation with owned `Arc<AtomicBool>` propagation; current import pipeline/task manager use `Arc<AtomicBool>`. | No open blocker; retain cancellation regression coverage when import workers are refactored. |
| M-02 | Medium | Artifact extraction used unbounded `read_to_end()`. | Stage 1 | Completed | Phase 1.4 records centralized `ARTIFACT_FILE_LIMIT_BYTES` and bounded reads. | Parser-specific artifact coverage can still be broadened, but the memory-limit risk is closed. |
| M-03 | Medium | Post-import pipeline collected file entries into large in-memory batches. | Stage 4 / Stage 5 | Completed for beta baseline | Phase 4.5 records import/search/artifact post-processing warning/skip/failure accounting; Stage 5 fixtures and tests cover representative pipelines. | Very large production images still need performance profiling and cursor/batch verification beyond tiny fixtures. Treat as a scalability risk, not a current security release blocker. |
| M-04 | Medium | Timeline/search/viewer paging had no hard maximum. | Stage 1 | Completed | Phase 1.5 records `PageRequest`, search, timeline, and viewer clamps with transport tests. | None known; new list endpoints must reuse transport paging/clamp DTOs. |
| M-05 | Medium | `get_file_tree` returned whole trees without pagination. | Stage 3 / Stage 5 | Completed for current UI path | Phase 3 file browser flow favors rows/children and targeted selection; final gates include file service tests. | Full-tree APIs remain a scalability-sensitive compatibility surface; future large-image work should prefer lazy children and add explicit max-count tests where legacy endpoints remain. |
| M-06 | Medium | `From<String> for CommandError` could bypass sanitization. | Stage 1 | Completed | Phase 1.5/1.6 and final grep gates record typed DTO/request validation and sanitized command errors; command SQL/error guard scripts reduce future boundary drift. | Continue avoiding blanket raw-string error conversions in new commands. |
| M-07 | Medium | DTO fields lacked input validation and range checks. | Stage 1, Stage 3 | Completed | Phase 1.1, 1.5, and 1.6 record import, paging, viewer, and media request validation/clamping. | Every new transport DTO needs explicit validation before command/service use. |
| M-08 | Medium | SQLite migrations could leave partial state or mark failed migrations as applied. | Stage 1 | Completed | Phase 1.7 records rollback/not-applied behavior and old-schema upgrade tests. | Continue adding migration-shape assertions for future table rebuilds. |
| M-09 | Medium | CSV report export was vulnerable to formula injection. | Stage 1, Stage 4 | Completed | Phase 4.6 records CSV formula sanitization regressions and `cargo test -p reports`. | None known; keep spreadsheet-export escaping in report tests when adding columns. |
| M-10 | Medium | DevTools/debug behavior lacked a release guard. | Stage 5 | Completed / guarded | Phase 5.3 records `scripts/check-release-guard.ps1`; final gate records it passing. | Guard must remain part of backend/release CI. |
| M-11 | Medium | Missing `event:default` capability could break frontend event listening. | Stage 4 | Completed | Phase 4.4 records capability alignment. | None known; new Tauri plugins/capabilities should be reviewed with event tests. |
| M-12 | Medium | CI lacked dependency audit and SBOM coverage. | Stage 5 | Completed / guarded | Phase 5.2 and 5.4 record `cargo audit`, `cargo deny`, deny-exception checks, EVTX dependency-decision guard, frontend audit, and CycloneDX backend/frontend SBOM artifacts. | `cargo audit` still reports allowed warning-class transitive advisories, mainly Tauri/GTK/WebKit/urlpattern/glib chains; track upstream upgrades and deny exception expiries. |

## Functional H/M remediation register

| ID | Original severity | Original finding | Remediation stage | Current status | Verification evidence | Residual risk / blocker |
|---|---:|---|---|---|---|---|
| F-01 | High | Reports page lacked export buttons/wiring despite registered backend export commands. | Stage 4 | Completed | Phase 0.2 records Reports export completion; Phase 4.6 records report provenance and exporter regressions. | None known. |
| F-02 | Medium | FileBrowser extract button had no handler. | Stage 3 | Completed | Phase 3.3 records `extract_file` command and save-dialog wrapper; frontend files tests are recorded in verification. | Destination validation should remain covered if extraction UX changes. |
| F-03 | Medium | FileBrowser “view in timeline” had no navigation logic. | Stage 3 | Completed | Phase 3.4 records selection and `/timeline` navigation wiring. | None known. |
| F-04 | Medium | Search “open in file browser” navigation was missing. | Stage 3 | Completed | Phase 0.2 records Search to Files behavior retained; Stage 3 navigation flows were closed. | None known. |
| F-05 | Medium | Import cancellation backend existed but no frontend cancellation UI. | Stage 4 | Completed | Phase 0.2 records cancel import UI; Phase 4.2 records typed `job-cancelled` event; task manager uses cancellable tokens. | Cancellation behavior on very long real imports should stay in manual/performance test scope. |
| F-06 | Medium | Several event topics had no backend emit path. | Stage 4 | Completed | Phase 4.2 records typed case/job/artifact/timeline/search/import/cancel events and frontend EventBus tests. | New topics must update Rust event constants, frontend `EventTopic`, Tauri bridge subscriptions, and tests together. |

## Stage 5 residual risk register

| Residual risk | Affected earlier stage(s) | Current severity | Why it remains | Required closure / blocker |
|---|---|---:|---|---|
| Registry parser is targeted, not a full registry browser. | Stage 2 | Medium | Current parser covers Analysis-required `SYSTEM`/`SOFTWARE` fields and synthetic tiny hive paths, not arbitrary production hive browsing. | Add production hive corpus, broaden parser cell/list coverage, and define UI/DTO contract for full registry browsing before claiming full Registry support. |
| Tiny E01 fixture is reader-level only. | Stage 2, Stage 5 | Medium | The committed 4,405-byte E01 fixture validates section/table/read/seek behavior but not a full E01 partition/filesystem import. | Run ignored/manual tests with `FORENSICS_E01_FIXTURE` against real E01 samples and record results. |
| WebView2 media seek has harness but lacks recorded manual result. | Stage 3, Stage 5 | Medium | CI guard proves protocol/CSP/fallback wiring; actual Windows WebView2 `<audio>/<video>` seek behavior requires desktop runtime observation. | Execute `scripts/run-webview2-media-smoke.ps1`, launch the Tauri app on Windows, complete the generated checklist, and record the result. |
| Dependency advisories remain warning-class and transitive. | Stage 5 | Medium | `evtx -> encoding`, direct `dirs`, and Tantivy old advisory chains were remediated, but Tauri/GTK/WebKit/urlpattern/glib chains still report allowed warnings. | Track upstream releases, expire/renew deny exceptions deliberately, and keep `cargo audit` / `cargo deny` in CI. |
| Backend coverage has artifact reporting but no percentage threshold. | Stage 5 | Medium | Rust coverage LCOV is generated when tools are available, but the baseline is not stable enough for a hard percentage gate. | Stabilize `cargo llvm-cov` availability and set crate/workspace thresholds once flaky/manual tests are isolated. |
| Full large-image scalability remains partially verified. | Stage 3, Stage 4, Stage 5 | Medium | Tiny fixtures protect correctness paths, but multi-million-entry imports and very large images require performance/profiling fixtures. | Add opt-in large fixture/performance jobs for import batching, file tree lazy loading, search indexing, and artifact extraction. |
| FS enum helper cleanup is partial quality debt. | Stage 5 | Low / Medium quality | Common root/path/error/child/truncation helpers exist, but deeper filesystem enumeration flow deduplication is still intentionally deferred. | Continue refactoring only with targeted FAT/exFAT/NTFS behavior tests to avoid changing public ordering/root semantics/reader behavior. |

## Verification blockers to keep visible

1. `FORENSICS_E01_FIXTURE` is not set in the default environment, so real E01 partition/filesystem slow tests remain ignored/manual.
2. WebView2 media smoke requires a Windows desktop runtime and cannot be closed by the protocol guard alone.
3. `cargo audit` warnings are allowed only while tracked by `deny.toml` exception metadata and upstream remediation plans.
4. Backend coverage thresholds should not be advertised as enforced until `scripts/run-coverage.ps1 -Rust -StrictRustTool` has a stable baseline.
5. Registry and large-image claims must distinguish tiny deterministic fixtures from production corpus validation.

## Recommended next verification commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
powershell -ExecutionPolicy Bypass -File scripts/check-release-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-command-sql-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-media-protocol-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-deny-exceptions.ps1
powershell -ExecutionPolicy Bypass -File scripts/check-evtx-dependency-decision.ps1
cargo deny check advisories bans licenses sources
cargo audit
pnpm --dir frontend audit --audit-level high
pnpm --dir frontend test:coverage
```

Manual/opt-in gates:

```bash
powershell -ExecutionPolicy Bypass -File scripts/run-webview2-media-smoke.ps1
cargo test -p app-services --test e01_full_pipeline_test -- --ignored --nocapture
cargo test -p app-services --test e01_mft_scan_test -- --ignored --nocapture
```
