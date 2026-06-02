# EVTX Dependency Decision

**Updated**: 2026-06-02 16:18:00 +08:00
**Owner**: Codex  
**Scope**: `artifacts-windows::evtx` boot/shutdown candidate adapter

## Resolved Decision

Forensics Workbench keeps the bounded `artifacts-windows::evtx` boot/shutdown
candidate adapter, but no longer consumes the crates.io `evtx -> encoding`
dependency path. The workspace now points `evtx = 0.11.2` at the local patched
fork in `crates/evtx-patched`, and that fork replaces the unmaintained
`encoding = 0.2.33` dependency with `encoding_rs`.

The previous temporary `RUSTSEC-2021-0153` exception has been removed from
`deny.toml`. `Cargo.lock` no longer contains a package named `encoding`, and the
artifacts-windows dependency graph now contains:

- `evtx v0.11.2 (D:\forensics\crates\evtx-patched)`
- `encoding_rs v0.8.x`

`scripts/check-evtx-dependency-decision.ps1` is now an anti-regression guard: it
fails if the workspace stops using `crates/evtx-patched`, if the local fork stops
depending on `encoding_rs`, if `Cargo.lock` reintroduces `encoding`, or if this
decision record stops documenting the patched fork.

## Adapter Boundaries

The parser is still a narrow forensic adapter, not a general EVTX platform:

- It reads at most 64 MiB from `System.evtx`.
- It emits only 6005, 6006, 6008, and 1074 boot/shutdown candidate events.
- It labels results as EventLog/User32 evidence, not as absolute
  boot/shutdown fact.
- Malformed, truncated, and oversized inputs produce warnings rather than
  fabricated records.
- A real `System.evtx` fixture covers the parser path.

The local fork also disables upstream sample-based crate tests and doctests
because the crates.io package excludes the referenced sample EVTX files. Runtime
coverage is provided by this workspace's `artifacts-windows` tests and the
committed tiny `System.evtx` fixture.

## Evidence

- `cargo tree -p artifacts-windows -i encoding` reports no package named
  `encoding`; Cargo only suggests `encoding_rs`.
- `cargo tree -p artifacts-windows -i evtx` points to
  `D:\forensics\crates\evtx-patched`.
- `cargo tree -p artifacts-windows | Select-String -Pattern
  "encoding|evtx|encoding_rs"` shows `evtx` through the local path and
  `encoding_rs`.
- `cargo audit` exits 0 and now reports 19 warning-class advisories; the
  previous `RUSTSEC-2021-0153` warning is absent.
- `cargo deny check advisories bans licenses sources` passes with existing
  duplicate dependency warnings only.
- `cargo test -p artifacts-windows evtx` passes 7 targeted EVTX tests.
- `cargo clippy -p evtx -p artifacts-windows --all-targets -- -D warnings`
  passes.

## Follow-Up

This decision removes the immediate `RUSTSEC-2021-0153` exception, but the
patched crate is still vendored source that must be maintained deliberately:

1. Re-check upstream `evtx` releases periodically and replace the local fork if
   a maintained permissive release removes the legacy dependency.
2. Keep the local fork minimal and avoid adding CLI, benches, or sample assets
   that are not needed by the bounded adapter.
3. Continue running the dependency guard, `cargo audit`, and `cargo deny` in CI.
4. Treat full EVTX parsing, broader event semantics, and additional fixture
   coverage as separate parser-roadmap work.
