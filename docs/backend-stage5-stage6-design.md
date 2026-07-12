# Backend Stage 5 and Stage 6 Implementation Design

## Scope

This document defines the executable design for the final two structural
refactor stages:

- Stage 5 reorganizes parser and core crates around one stable capability per
  production file.
- Stage 6 moves every Rust test body out of production `src/` trees.

Both stages are behavior-preserving. They must not change parser algorithms,
expected JSON, evidence addressing, source database isolation, platform
routing, Tauri command names, or read-only evidence semantics.

## Development Baseline

The implementation baseline is commit `49561c9a`.

Locked facts at the start of Stage 5:

- Module-size migration baseline: 83 debt rows.
- Function-size migration baseline: 65 debt rows.
- Test-layout migration baseline: 206 debt rows.
- Windows and Linux are the only production analysis platforms.
- Multi-source data is routed through one control database and independent
  source databases.
- Real-sample isolation is validated with `检材2.E01` and `检材3.E01`.
- PVE cluster support remains outside the completion boundary of these stages.

## Shared Engineering Boundaries

- A production file owns one parser, reader, decoder, projection, or format
  capability.
- `lib.rs` and `mod.rs` contain declarations, public facade types, and
  re-exports only.
- Existing public crate APIs remain available through facade re-exports.
- Internal seams use `pub(super)` or `pub(crate)` unless they are already part
  of a documented public API.
- No production API may be widened solely to make a test compile.
- Parser ordering, error classification, bounds checks, sparse-file behavior,
  offset translation, and recovery behavior remain unchanged.
- Production files target 500 lines and must not introduce new files above 800
  lines. Module roots must not introduce new files above 200 lines.
- New or moved production functions target 100 lines and may not exceed 150
  lines.
- Files are UTF-8 without BOM and use LF line endings.
- Cargo validation is serial with `CARGO_BUILD_JOBS=1`.

## Stage 5 - Parser and Core Decomposition

### Stage Design

Stage 5 separates implementation structure from algorithm behavior. Large
parser files are decomposed by format unit and reader responsibility while the
crate facade preserves existing imports. The stage is implemented in disjoint
families and integrated only after crate-level regression tests pass.

### Phase 5.1 - Linux Filesystem Family

Tasks:

- Split `fs-btrfs` into superblock, chunk mapping, tree traversal, inode/file
  reading, compression, checksum, directory, and filesystem facade modules.
- Split `fs-ext4` into superblock/group metadata, inode/extents, directory,
  file reading, feature validation, and filesystem facade modules.
- Split `fs-xfs` into superblock/AG metadata, inode, extent/bmap, file reading,
  directory dispatch, and filesystem facade modules.
- Split `fs-lvm` into PV/VG discovery, metadata areas, logical-volume mapping,
  segment mapping, thin-pool metadata, and reader modules.
- Preserve the XFS and LVM fixes validated against `检材3.E01`.

Expected result:

- Filesystem crate roots become API facades.
- Logical-to-physical offset calculations remain byte-for-byte equivalent.
- Sparse, unwritten, compressed, thin-provisioned, and recovery paths preserve
  current behavior.

### Phase 5.2 - Windows Artifact Parser Family

Tasks:

- Split EVTX parsing into header/chunk discovery, record iteration, template
  decoding, value conversion, and recovery modules.
- Split Firefox extraction into profile discovery, history/downloads,
  bookmarks, cookies, logins, and shared SQLite mapping modules.
- Split Registry SAM parsing into structure decoding, account projection,
  cryptographic material handling, and lookup orchestration.
- Keep fixture and expected JSON contracts unchanged.

Expected result:

- Each parser family has a small facade and single-purpose implementation
  files.
- Existing artifact records, source object IDs, timestamps, and deterministic
  ordering remain unchanged.

### Phase 5.3 - Container, Query, and Linux Artifact Family

Tasks:

- Split mbox parsing into framing, header decoding, MIME projection, attachment
  handling, and container iteration modules.
- Split GQL parsing into lexer/token stream, expression parsing, clause
  parsing, AST validation, and diagnostics.
- Split Linux journal parsing into header/object reading, entry traversal,
  field decoding, and artifact projection.
- Preserve public parser entry points and typed errors.

Expected result:

- Container and query facades remain source compatible.
- No parser output, diagnostic category, or extraction limit changes.

### Phase 5.4 - Guard and Documentation Integration

Tasks:

- Add a Stage 5 parser boundary guard for facade size, known parser roots, and
  forbidden test bodies in newly split production modules.
- Remove only resolved module/function baseline rows.
- Update the backend architecture document and documentation index.
- Run crate-level tests after each family and workspace tests after integration.

## Stage 5 Test Matrix

| Area | Required validation |
|---|---|
| Filesystem public API | Existing imports compile without caller changes |
| Offset correctness | LVM, XFS, ext4, and Btrfs block-to-byte mapping remains stable |
| Corruption handling | Invalid magic, truncated metadata, bad checksums, and unsupported features retain typed errors |
| Windows artifacts | Existing EVTX, Registry, and Firefox fixtures match expected JSON |
| Containers/query | mbox and GQL valid, invalid, and edge fixtures retain results |
| Linux artifacts | journal records and field projection retain counts and values |
| Real Linux sample | `检材3.E01` file tree and arbitrary-file preview do not regress |
| Source isolation | Windows/Linux serial dual-source guard still passes in both orders |
| Performance | Import, browse, preview, and extraction do not regress by more than 10% |

## Stage 5 Review Gate

The independent review scores architecture 25, modularity 20, contract
preservation 15, robustness 15, tests 15, and performance 10.

Stage 5 cannot be committed unless:

- Total score is at least 90.
- Every dimension is at least 80%.
- No High or Critical finding remains.
- Format, clippy, workspace tests, repository guards, and real-sample isolation
  pass.

## Stage 6 - Physical Test Separation

### Stage Design

Stage 6 removes test implementation from every production source tree. Private
unit tests are compiled through exact, non-public bridges to physical
`tests/unit/` files. Public integration scenarios remain normal Cargo
integration tests. The migration is organized by crate family so private
visibility remains controlled and merge conflicts stay bounded.

### Phase 6.1 - Filesystem Crates

Tasks:

- Move inline and source-local tests from `fs-xfs`, `fs-lvm`, `fs-ext4`,
  `fs-btrfs`, `fs-ntfs`, `fs-exfat`, and `fs-fat`.
- Move `src/tests.rs`, `src/*_tests.rs`, and test-only helpers into
  `tests/unit/` or `tests/support/`.
- Retain only exact `#[cfg(test)]`, `#[path = "..."]`, `mod tests;` bridges
  where private access is required.

### Phase 6.2 - Parser and Artifact Crates

Tasks:

- Move tests from `artifacts-windows`, `artifacts-linux`, `containers-pst`,
  and `gql`.
- Keep fixtures and expected JSON under their existing authoritative fixture
  locations.
- Avoid test-only visibility changes in parser modules.

### Phase 6.3 - Core, Transport, and Persistence Crates

Tasks:

- Move tests from `transport`, `timeline`, `exchange`, `domain`,
  `evidence-core`, image crates, persistence, MCP, ingest, updater, and
  remaining workspace members.
- Place reusable builders in physical `tests/support/` or the existing
  `testing` crate.
- Keep top-level `tests/*.rs` files as Cargo integration entries, never bridge
  targets.

### Phase 6.4 - Zero-Debt Guard Integration

Tasks:

- Add a Stage 6 guard that requires zero test bodies beneath production
  `src/`.
- Reduce `rust-test-layout-baseline.csv` to its exact header.
- Update architecture and testing documentation.
- Run the complete workspace and repository quality gates.

## Stage 6 Test Matrix

| Area | Required validation |
|---|---|
| Physical layout | No `#[test]`, inline test module, or test helper body remains under `src/` |
| Bridge safety | Every bridge resolves canonically inside the owning `tests/unit/` tree |
| Visibility | No public API is added solely for tests |
| Test discovery | Workspace test count does not unexpectedly decrease |
| DTO contracts | Serde round-trip tests continue to execute |
| Parser coverage | Valid, invalid, edge, fixture, and expected JSON tests remain active |
| Integration | Source DB isolation, import, preview, analysis, and reporting tests pass |
| Real samples | `检材2.E01` and `检材3.E01` isolation passes in both serial orders |
| Documentation | Test layout, commands, and baseline state match executable guards |

## Stage 6 Review Gate

Stage 6 uses the same scoring model as Stage 5. It cannot be committed unless:

- Test-layout debt is zero and the baseline is header-only.
- Workspace test discovery is preserved.
- Total score is at least 90 and every dimension is at least 80%.
- No High or Critical finding remains.
- All default quality gates pass.

## Final Acceptance Criteria

- Parser and core production files have clear, single-capability ownership.
- Parser crate facades contain declarations and re-exports rather than large
  implementations.
- No parser algorithm, DTO, evidence reader contract, expected JSON, or Tauri
  command changes as a consequence of the refactor.
- No Rust test body or test-only helper remains beneath a production `src/`
  tree.
- No production public API was expanded solely for testing.
- Module, function, and test-layout guards pass with only genuinely unresolved
  pre-existing debt; Stage 6 test-layout debt is exactly zero.
- Windows/Linux multi-source isolation and the `检材3.E01` Linux regression
  remain green.
- Stage 5 and Stage 6 each receive an independent review and separate commit.
