# EVTX Dependency Decision

**Updated**: 2026-07-26
**Owner**: Codex  
**Scope**: `artifacts-windows::evtx` structured-event adapter and local EVTX fork

## Resolved Decision

Meow~Detective keeps the bounded `artifacts-windows::evtx` structured-event
adapter, but no longer consumes the crates.io `evtx -> encoding`
dependency path. The workspace now points `evtx = 0.11.2` at the local patched
fork in `crates/evtx-patched`, and that fork replaces the unmaintained
`encoding = 0.2.33` dependency with `encoding_rs`.

The previous temporary `RUSTSEC-2021-0153` exception has been removed from
`deny.toml`. `Cargo.lock` no longer contains a package named `encoding`, and the
artifacts-windows dependency graph now contains:

- `evtx v0.11.2` from the workspace path `crates/evtx-patched`
- `encoding_rs v0.8.x`

`scripts/check-evtx-dependency-decision.ps1` is now an anti-regression guard: it
fails if the workspace stops using `crates/evtx-patched`, if the local fork stops
depending on `encoding_rs`, if `Cargo.lock` reintroduces `encoding`, or if this
decision record stops documenting the patched fork.

The workspace enables the fork's `multithreading` feature. The application
configures at most four parser workers: EVTX chunks are read sequentially from
the evidence reader and each bounded batch is parsed in parallel. This does not
introduce concurrent evidence reads and bounds parser memory to a small number
of 64 KiB chunks plus their decoded records.

The local fork also replaces `Read::read_to_end` chunk loading with an explicit
64 KiB read loop. An `Interrupted` evidence read is preserved as
`ChunkError::FailedToReadChunk` and mapped by the adapter to cancellation; it is
never retried indefinitely by the standard-library helper. Physical chunk
numbers are carried through parallel batches so diagnostics report the actual
EVTX offset rather than a batch-local index.

## Adapter Boundaries

The parser is still a narrow forensic adapter, not a general EVTX platform:

- Seekable evidence readers parse the complete EVTX stream without first
  copying the entire file into memory.
- Structured records are visited incrementally. The application projects and
  writes at most 256 records per batch inside one candidate transaction, and
  stores the output checkpoint in that same transaction.
- Candidate output digests use a bounded, order-independent accumulator rather
  than collecting and sorting every projected record in memory.
- Non-seekable fallback readers are limited to 16 MiB and reject larger EVTX
  candidates with an explicit warning.
- It persists only the event IDs and channel families listed by
  `artifacts-windows::evtx::SUPPORTED_EVENT_IDS`; it is not a general-purpose
  event-message renderer.
- Boot and shutdown results remain EventLog/User32 evidence, not absolute
  machine-state facts.
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
  the workspace-local `crates/evtx-patched` package.
- `cargo tree -p artifacts-windows | Select-String -Pattern
  "encoding|evtx|encoding_rs"` shows `evtx` through the local path and
  `encoding_rs`.
- `cargo audit` exits 0 and now reports 19 warning-class advisories; the
  previous `RUSTSEC-2021-0153` warning is absent.
- `cargo deny check advisories bans licenses sources` passes with existing
  duplicate dependency warnings only.
- `cargo test -p artifacts-windows evtx` covers the real public EVTX fixture,
  buffered limits, seekable full-stream parsing, malformed input, and curated
  event projection.
- `cargo test -p evtx --test serialized_records` locks physical chunk identity
  across skipped chunks and parallel batches.
- `cargo test -p app-services seekable_evtx --lib` covers checkpoint replay and
  rollback of an injected persistence failure.
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
4. Treat broader event semantics and additional channel fixtures as separate
   parser-roadmap work.
