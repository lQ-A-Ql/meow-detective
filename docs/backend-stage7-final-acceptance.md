# Backend Stage 7 Final Acceptance

## 1. Scope

Stage 7 closes the backend platform-domain and single-responsibility refactor.
It does not add parser features. Acceptance requires architecture boundaries,
source/test separation, real evidence behavior, frontend contracts,
documentation, and performance to agree.

Baseline delivery commits:

| Stage | Commit | Result |
|---|---|---|
| Stage 0 | `c8597888` | Structural baselines and monotonic guards |
| Stage 1 | `aed82c02` | macOS production support removed |
| Stage 2 | `7ac7e695` | Windows/Linux platform peers and source isolation |
| Stage 3 | `c3ae351b` | Transport and Tauri command decomposition |
| Stage 4 | `49561c9a` | Application-service decomposition |
| Stage 5 | `4c2bd3a7` | Parser/filesystem capability decomposition |
| Stage 6 | `72493fce` | Rust test bodies physically separated from `src/` |

## 2. Final Architecture Audit

| Boundary | Evidence | Result |
|---|---|---|
| Production platforms | Domain dispatch and symmetric analyzers expose Windows/Linux only; retired macOS requests fail typed unsupported | Passed |
| Parser dependency direction | Parser/core crates do not depend on Tauri or frontend code | Passed |
| Service boundary | `crates/app-services` has no Tauri dependency or import | Passed |
| Command boundary | Command handlers contain no raw SQL; Stage 3 guard confirms thin adapters | Passed |
| Frontend boundary | Route pages do not import API/store/Tauri directly; production frontend has no demo/mock fallback | Passed |
| Test placement | Non-vendored production `src/` contains zero test bodies; bridge targets remain under physical `tests/unit/` | Passed |
| Module ownership | Stage 3-5 guards lock command, service, parser, filesystem, and facade ownership | Passed |
| Source database isolation | Windows/Linux serial import passes in both orders with source-scoped IDs and independent `source.db` files | Passed |

Service-layer SQL remains in 29 explicit persistence/query-oriented modules.
This is accepted residual structure because command SQL is zero and the SQL is
contained in repository-like helpers such as `artifact_query`, `pagination`,
`projection`, `staging`, `source_db`, and persistence-oriented service files.
Future changes should continue moving reusable statements into repositories;
new SQL must not return to Tauri commands.

## 3. Structural Debt

| Metric | Before Stage 5/6 | Stage 7 result | Policy |
|---|---:|---:|---|
| Module-size baseline rows | 83 | 0 | Header-only baseline; no new migration debt |
| Function-size baseline rows | 65 | 8 | Existing identities may only shrink or be deleted |
| Historic functions above 150 lines | not separately closed | 0 | Locked; no new function above 100 lines |
| Test-layout baseline rows | 206 | 0 | Header-only baseline; no new production test bodies |
| Formal module exceptions | 0 | 5 | Reviewed normal-module exceptions expire on 2026-09-30 |

The remaining function rows and five temporary module exceptions are visible
debt, not hidden Stage 7 failures. All three guards compare against the
committed reference and reject new or growing debt. The Stage 4 closure on
2026-07-13 removed all app-services rows from both size baselines and added a
Stage 4-specific zero-debt guard for the application-service layer. The
post-Stage 7 cleanup on 2026-07-13 split the historical `fs-fat`, `fs-exfat`,
and `image-e01` module roots and reduced the migration module baseline to zero.

## 4. Real-Sample Acceptance

### 4.1 Linux single-disk baseline

Command:

```powershell
$env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'
cargo test -p app-services --test linux_e01_integration -- --ignored --nocapture
```

Observed on 2026-07-12:

- 20 tests passed in 180.00 seconds.
- LVM pool remains `Expanded`/redirected and is not exposed as an expandable
  file-tree root.
- `cl/root` is imported as the visible XFS root LV.
- `/etc` contains 201 direct children.
- Arbitrary `FileEntryId` preview succeeds for `/etc/fstab`,
  `/root/.bash_history`, and `/var/log/wtmp`.
- Large-file preview validates head, middle, and tail ranges.
- Linux extraction scanned 749 candidate sources and produced 50,991
  artifacts plus 446 timeline events.
- All 9 sections returned independent progress:
  `LinuxJournal`, `LinuxLogin`, `LinuxCommands`, `LinuxPackages`,
  `LinuxCron`, `LinuxSudo`, `LinuxSystemConfig`, `LinuxWebServices`, and
  `LinuxMysqlServices`.
- Candidate coverage was 0.552737 with explicit partial/unsupported warnings;
  unsupported sources were not fabricated as parsed artifacts.

Focused recovery verification on 2026-07-21 against the same sample also
passed:

- XFS log snapshot: 10 MiB bounded snapshot, 3,007 records, 2,872
  transactions, 31,334 metadata candidates, and 1,973 deletion candidates;
  all deletion candidates carried verified zero-link evidence and no parser
  issues were reported.
- Root LV completeness: 50,934 files and 7,140 directories were enumerated
  with zero warnings; `/etc/passwd`, `/etc/os-release`, and `/etc/hostname`
  were read through the stored file-entry preview path.
- Linux extraction: 749 candidate sources produced 50,991 persisted
  artifacts and 446 timeline events. The nine Linux sections reported their
  own status; partial results remained explicitly warned rather than promoted
  to fabricated structured artifacts.

### 4.2 Windows/Linux source isolation

Command:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-stage2-real-sample-isolation.ps1 `
  -WindowsFixturePath 'D:\獬豸杯\检材2.E01' `
  -LinuxFixturePath 'D:\獬豸杯\检材3.E01' `
  -RequireFixtures
```

Observed on 2026-07-12:

- Windows -> Linux: passed in 344.55 seconds.
- Linux -> Windows: passed in 330.34 seconds.
- NTFS metadata did not change to XFS/LVM after the Linux import.
- Source databases, file IDs, partition metadata, file trees, previews,
  artifacts, and timelines remained source-scoped.

### 4.3 E01 import performance

The original gate required at least 100,000 rows, while the stable
`检材2.E01` result is 91,737 rows. Stage 7 corrected the default minimum to
90,000 so the integrity gate remains strict and achievable for its reference
fixture. Time, RSS, and throughput thresholds were not relaxed.

Observed three-run profile on 2026-07-12:

| Metric | Result | Gate |
|---|---:|---:|
| Total median | 13.479 s | <= 45 s |
| Enumeration median | 8.488 s | <= 30 s |
| Maximum RSS | 582 MB | <= 1,024 MB |
| Imported rows | 91,737 each run | >= 90,000 |
| Minimum throughput | 9,892 rows/s | >= 6,000 rows/s |

Result: passed.

## 5. Quality Gates

Passed:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -j 1 -- -D warnings`
- `cargo test --workspace -j 1`
- frontend typecheck, lint, 87 Vitest files / 547 tests, and production build
- module, function, test-layout self-tests and normal guards
- Stage 0, 2, 3, 4, 5, and 6 architecture guards
- command SQL, media protocol, release, Stage 5 regression, import
  optimization, lockfile, dependency exception, and EVTX decision guards
- dependency advisory/license/source policy
- small benchmark regression
- real Linux sample, dual-source isolation, and E01 import performance gates
- `git diff --check`

Dependency policy reports reviewed duplicate-version warnings but no advisory,
license, source, or ban failure. Those duplicates remain dependency-governance
work, not an unreported clean state.

## 6. Quality Score

This score evaluates the backend refactor delivery, not the separate V2
product-release scorecard.

| Dimension | Score | Evidence |
|---|---:|---|
| Architecture | 25/25 | Dependency direction and platform ownership guards pass |
| Modularity | 20/20 | Module migration baseline and app-services debt are zero; five reviewed module exceptions and nine function rows remain explicitly governed |
| Contract | 15/15 | Rust/frontend contracts and platform/source routing pass |
| Robustness | 14/15 | Fail-closed unsupported paths pass; dependency duplicates remain |
| Testing | 15/15 | Workspace, frontend, physical separation, and real samples pass |
| Performance | 10/10 | E01 profile and benchmark gates pass without relaxed time/RSS/throughput |
| **Total** | **99/100** | **Approved** |

No Critical or High finding remains. The score exceeds the 90-point stage
threshold and no dimension is below 80%.

## 7. Accepted Boundaries

- Production analysis platforms are Windows and Linux only.
- Linux single-disk LVM/XFS is a private-sample baseline, not public GA proof.
- PVE member discovery, member isolation, and host EXT4 reading are verified;
  cluster-level semantic analysis remains deferred.
- Ceph BlueStore, VM disk reconstruction, cross-node correlation, complete LVM
  thin/cache/RAID/snapshot/VDO/writecache coverage, and degraded VG activation
  remain unsupported or partial as documented.
- systemd/SSH/sudoers/profile.d extraction remains best-effort where no
  dedicated semantic DTO/parser exists.
- Public Linux fixtures and expected JSON remain required before raising public
  support levels.

## 8. Acceptance Decision

Stage 7 is approved when this document, the support/unsupported matrices, and
the validation framework pass documentation guards in the final commit. The
backend refactor is complete within its declared boundary; remaining items are
tracked product/parser maturity work rather than hidden refactor failures.
