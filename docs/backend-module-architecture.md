# Backend Module Architecture

This document defines the Stage 0 backend refactor guardrails for Rust modules
and tests. It is intentionally mechanical: the rules are written so that guard
scripts can enforce them during an incremental migration without touching
business source code.

## Scope

The backend source layout is discovered from
`cargo metadata --no-deps --format-version 1`. Every non-vendored workspace
member is covered, including members outside the conventional `crates/`
directory. Typical roots are:

- `crates/*/src/**/*.rs`
- `apps/desktop/src-tauri/src/**/*.rs`
- non-vendored workspace `build.rs` files

Only physical top-level Cargo `tests/`, `benches/`, and `examples/` trees are
excluded from production module/function limits. Every `.rs` file beneath an
owning package's `src/` is scanned, including `tests.rs`, `*_tests.rs`,
`test_helpers.rs`, and `src/tests/**`. The vendored `crates/evtx-patched`
workspace member is explicitly excluded. The function guard does not infer
exclusions from arbitrary directory names such as `vendor/` or `generated/`; a
new exclusion requires an explicit architecture decision.

Cargo target source files are scanned regardless of extension, but every
production target must remain inside its owning package's `src/`; the exact
package-root `build.rs` is the only exception. Broad package-root sibling
scanning is intentionally not used. A workspace member nested below another
member's `src/` is rejected as an unsupported ambiguous ownership layout.
Physical file identities follow host filesystem semantics: Windows paths are
deduplicated case-insensitively while Linux paths remain case-sensitive, and
the stable output path comes from the recursive `src` enumeration.
Production `#[path] mod ...` and token-injecting `include!` are rejected;
`include_str!` and `include_bytes!` remain data includes. Missing,
cross-package, repository-external, directory, or reparse-point target paths
fail closed.

## Platform Domains

Platform-specific artifact capabilities are symmetric peers. Windows and Linux
domains should have equivalent ownership rules, naming conventions, and test
placement:

- Windows artifact parsing belongs in `crates/artifacts-windows/src/`.
- Linux artifact parsing belongs in `crates/artifacts-linux/src/`.
- Cross-platform orchestration belongs in `crates/app-services/src/` and must
  call platform crates through explicit capability boundaries.
- Platform-neutral DTOs stay in `crates/transport/src/dto/`; platform-specific
  DTO files may exist, but the naming must make the platform boundary obvious.

Do not hide platform-specific parsing inside app-services, transport, or the
Tauri command layer. Windows and Linux use the same architectural layer and
ownership rules, but a platform module is created only for a real capability;
do not manufacture unsupported stubs merely to make directory trees symmetric.

`domain::DataSourcePlatform` is the application platform type. Transport
platform DTOs are converted at the desktop command boundary and must not enter
`app-services`. Persisted strings are parsed through the domain type before
platform dispatch; retired or unknown external values fail closed.

Analysis orchestration uses symmetric platform analyzers:

```text
analysis_service/
  capability.rs
  platforms/
    windows.rs
    linux.rs
  candidates/
    common.rs
    windows.rs
    linux.rs
    summary.rs
  extraction/
    linux/
      journal.rs
      login.rs
      shell_history.rs
      packages.rs
      cron.rs
      sudo.rs
      system_config.rs
      pve.rs
      web.rs
      mysql.rs
      text_log.rs
```

Capability keys, platform ownership, section labels, candidate category, and
read policy are centralized in `analysis_service/capability.rs`. Empty category
selection means all capabilities for the persisted source platform, never all
platforms. A cross-platform category request is rejected before evidence I/O.

Ordinary data-source imports are also source-isolated during LVM probing. They
must never enumerate other case data sources as supplementary PV providers.
Cross-image multi-PV aggregation remains unavailable until cluster members can
be registered and validated atomically; an incomplete VG fails closed instead
of borrowing blocks from an unrelated `source.db` registration.

Case-wide file-tree, recent-object, search, metrics, step-recording, artifact,
timeline, graph, and correlation reads use the shared ready-source router. Only
the exact `ready` import state is readable; `pending`, `importing`, and `failed`
sources are excluded so partial imports cannot leak into analysis or reports.
Case-aware reports must also use the source-aware governance snapshot, keeping
governance runtime signals aligned with the source-database correlation section.

## One Capability Per File

Production Rust files should be owned by one capability:

- A parser file owns one parser family or one closely related format unit.
- A repository file owns one aggregate or table family.
- A service file owns one use-case family.
- `mod.rs` and `lib.rs` are public API and re-export surfaces, not warehouses
  for implementation or large inline tests.

Line budgets:

- Normal production file target: 500 lines.
- Normal production file hard ceiling for new debt: 800 lines.
- `mod.rs` and `lib.rs` hard ceiling for new debt: 200 lines.
- Function target: 100 lines; 150-line hard ceiling for new debt.

The executable Stage 3 and Stage 4 split plan is maintained in
`docs/backend-stage3-stage4-design.md`. It defines transport request domains,
the thin Tauri command boundary, application-service target modules, review
scoring, the regression matrix, and performance acceptance criteria.

The executable Stage 5 and Stage 6 plan is maintained in
`docs/backend-stage5-stage6-design.md`. It defines parser/core capability
families, behavior-preserving facade rules, physical test migration order,
real-sample regressions, review scoring, and zero-test-debt acceptance.

Stages 5 and 6 are complete at commits `4c2bd3a7` and `72493fce`.
Production parser/core files are organized by capability family, and the
non-vendored `src/` test-layout baseline is now header-only with zero test
bodies. Stage 7 final evidence, residual debt, quality scoring, and acceptance
commands are recorded in `docs/backend-stage7-final-acceptance.md`.

The module, function, and test-layout limits are automated in this Stage 0
slice. The function guard uses a compiled lexer so comments, nested block
comments, ordinary/byte/raw strings, character literals, closures, and nested
macro braces cannot terminate a function span. It supports free, impl, trait,
default, async, unsafe, extern, visibility-qualified, and multiline-signature
functions. A span starts at the first attached attribute/visibility/modifier
token found by `FindDeclarationStart` and ends at the declaration semicolon or
matching body brace, both inclusive. Const-generic brace groups are skipped as
balanced units before angle-depth processing, so comparisons such as
`Assert<{ N < 64 }>` cannot hide the function body.

Test-only exclusion is conservative. The scanner evaluates `cfg` expressions
only far enough to prove that enabling the item implies `test=true`:
`cfg(test)`, `cfg(any(test))`, `cfg(all(test, ...))`, and nested combinations
with the same implication are excluded. Mixed alternatives such as
`cfg(any(test, feature = "x"))`, `cfg(not(test))`, unknown predicates, and
complex negations are retained. This may over-count ambiguous test-gated code,
but it cannot silently remove production-capable functions from the guard.

During migration, existing file violations are tracked in
`scripts/baselines/rust-module-size-baseline.csv`, and existing functions over
100 lines are tracked in `scripts/baselines/rust-function-size-baseline.csv`.
Function identities combine normalized path, function name, normalized
signature SHA-256, and same-signature occurrence. This remains stable across
formatting and line movement while rejecting renamed, moved, or replaced
functions. A tracked violation may stay the same or shrink, but it must not
grow. Existing debt above 150 lines remains migration-baselined and must shrink;
the guard rejects every non-baselined function above 100, so new code cannot
approach or cross the 150-line hard ceiling. Stale, duplicate, malformed, or
non-deterministically ordered rows fail validation. When migration is complete,
each migration baseline is reduced to its exact CSV header; an empty file is
invalid.

Migration baselines cannot authorize themselves in the same change. Normal
guard runs compare them with the committed baseline at `-ReferenceRevision`,
then the guard-specific `RUST_*_BASELINE_REFERENCE` environment variable, then
local `HEAD` in that order. A transition may only lower debt metrics or delete
an identity; adding a baseline path/identity, raising an allowance, or changing
identity metadata fails. CI supplies the PR base SHA (or previous push SHA) and
checks out full history. Each first Stage 0 bootstrap requires three values to
agree: the generated baseline bytes, its
`scripts/baselines/rust-*-bootstrap.csv` audit record, and a protected
repository variable supplied outside the pull request. The variables are
`RUST_MODULE_SIZE_BOOTSTRAP_SHA256`,
`RUST_FUNCTION_SIZE_BOOTSTRAP_SHA256`, and
`RUST_TEST_LAYOUT_BOOTSTRAP_SHA256`. A missing or mismatched protected value
fails closed, so changing a baseline and its manifest in one pull request
cannot self-authorize. Bootstrap authorization is consulted only when the
reference revision has no corresponding baseline.

## Physical Test Separation

Tests should live outside production source bodies:

- Preferred crate integration entry: `crates/<crate>/tests/integration.rs`, with
  scenarios grouped under `tests/integration/`.
- Preferred Tauri integration entry: `apps/desktop/src-tauri/tests/integration.rs`.
- Private unit-test bodies included from `src` belong under `tests/unit/` so
  Cargo does not compile them again as top-level integration crates.
- Shared test helpers belong under a `tests/` tree or a dedicated test-support
  crate, not in production `src` modules.

After migration, the only allowed test bridge inside `src` is an external module
declaration that points into the owning physical `tests/unit/` directory:

```rust
#[cfg(test)]
#[path = "../tests/unit/capability.rs"]
mod tests;
```

Nested source files should use the correct relative path, but the normalized
path is resolved relative to the declaring source file, must exist, and its
canonical path must remain inside that crate/app's physical `tests/unit/`
directory. A bridge to a top-level `tests/*.rs` file is rejected because Cargo
would also compile that file as a separate integration crate.
The module name must be exactly `tests` and must not be public. Inline
`#[cfg(test)] mod ... { ... }`
bodies, `mod tests { ... }`, `#[test]` attributes, and `#[cfg(test)]` helpers
inside `src` are migration debt.

Existing debt is tracked in
`scripts/baselines/rust-test-layout-baseline.csv` with per-file ceilings for:

- Inline test module count.
- Inline test module line count.
- Test attribute count.
- `mod tests {` block count.
- `#[cfg(test)]` helper item count outside inline modules.
- Physical test-only file lines under `src`.

A tracked metric may stay the same or shrink, but it must not grow. New source
test debt fails the guard. When migration is complete, the baseline is reduced
to its exact CSV header; an empty file is invalid.

## Guards

Run these Stage 0 backend guards locally before opening refactor PRs:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-module-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-module-size.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-rust-test-layout.ps1 -SelfTest
powershell -ExecutionPolicy Bypass -File scripts\check-rust-test-layout.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage0-boundary-guard.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage2-platform-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage2-real-sample-isolation.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage3-command-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage4-service-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage5-parser-boundary.ps1
powershell -ExecutionPolicy Bypass -File scripts\check-stage6-test-separation.ps1
```

The guards are PowerShell 5.1 compatible, read files as strict UTF-8, and report
repository-relative paths normalized with `/`.
`scripts/lib/RustGuard.Common.ps1` is the single policy implementation for
Cargo workspace discovery, production/test file boundaries, ordinal path and
CSV validation, and reparse-point rejection.
`cargo metadata` captures stdout/stderr asynchronously and times out after 30
seconds by default. `RUST_GUARD_METADATA_TIMEOUT_MS` may set a reviewed value
from 100 through 300000 milliseconds. On Windows, timeout cleanup uses an
exact-PID `taskkill`, a kill-on-close Job Object, and a bounded PID/parent/
creation-time snapshot fallback; self-tests force each fallback and prove a
long-lived child does not survive. It never terminates processes by name.
Timeout errors explicitly identify a possible package-cache or build-directory
lock.

Test-layout alias analysis reaches a fixed point for explicit `use ... as ...`
chains in one source file. It intentionally does not expand unknown procedural
macros or resolve cross-file wildcard/re-export graphs. Such test frameworks
require an explicit architecture decision and a guard update before use; they
must not be used to place hidden test bodies in `src`.

To regenerate a proposed baseline, print CSV to stdout and apply the resulting
file changes with `apply_patch`:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-module-size.ps1 -GenerateBaseline
powershell -ExecutionPolicy Bypass -File scripts\check-rust-function-size.ps1 -GenerateBaseline
powershell -ExecutionPolicy Bypass -File scripts\check-rust-test-layout.ps1 -GenerateBaseline
```

The scripts intentionally do not write baseline files.

## Exceptions

Exceptions are narrow and temporary:

- Third-party or vendored `crates/evtx-patched` source is not scanned or recorded
  in migration baselines.
- Parser constants and static format tables may remain near their parser, but
  they do not exempt a file from the line guard.
- A module may keep an external `#[cfg(test)]` bridge only in the exact
  three-line form shown above and only after canonical `tests/unit/**` path
  validation succeeds. Any reparse point from the owning workspace member to
  the target fails closed.
- New normal modules between 501 and 800 lines require a temporary entry in
  `scripts/baselines/rust-module-size-exceptions.csv` with non-empty
  `path/owner/reason/expires`. Duplicate, expired, stale, missing, invalid, or
  migration-baseline-overlapping entries fail the guard.
- Function-size exceptions do not exist. Every existing function above 100
  lines requires an exact migration baseline identity and may only shrink.
  Existing >150-line debt remains visible and locked; new non-baselined >100
  functions fail, with 150 retained as the hard ceiling for new debt.
- A baseline transition never accepts new rows. Each one-time bootstrap must pin
  the pre-baseline commit and exact initial baseline SHA-256 in both the audit
  manifest and a protected repository variable outside the pull request.
- Business-source edits must not be made only to satisfy Stage 0 baseline
  generation. Stage 0 records the current debt; later stages perform the moves.

If a future exception is required, document the owner, reason, and removal plan
near the relevant refactor plan before changing guard behavior.
