# EVTX Dependency Decision

**Updated**: 2026-06-02 11:45:00 +08:00  
**Owner**: Codex  
**Scope**: `artifacts-windows::evtx` boot/shutdown candidate adapter

## Current Decision

Forensics Workbench currently keeps `evtx = 0.11.2` behind a bounded adapter
for `System.evtx` boot/shutdown candidates. The adapter reads at most 64 MiB,
emits only 6005/6006/6008/1074 candidates, and labels the result as EventLog or
User32 evidence rather than direct boot/shutdown fact.

The dependency exception for `RUSTSEC-2021-0153` remains temporary. `evtx
0.11.2` directly depends on `encoding = 0.2.33`, and that dependency is not
feature-gated. `deny.toml` tracks the exception with owner, reason, and expiry
date `2026-09-01`; `scripts/check-deny-exceptions.ps1` fails once the exception
expires.

## Evidence

- `cargo search evtx` reports `evtx = 0.11.2` as the current crates.io release.
- Upstream `EVTX` commit `38a2d50b21629edb3dd77953a2c02a4b944badf1` still has
  `version = "0.11.2"` and `encoding = "0.2.33"` in `Cargo.toml`.
- Local dependency path is `encoding v0.2.33 -> evtx v0.11.2 ->
  artifacts-windows`.
- `encoding` is referenced in `evtx` parser/binxml/render paths through
  `EncodingRef`, `WINDOWS_1252`, and `DecoderTrap`, so removing it is not a
  one-line feature change.
- GPL alternatives inspected in this pass are not acceptable drop-in
  replacements for this MIT workspace:
  - `evtx-msg 1.0.1`: GPL-3.0.
  - `exhume_artefacts 0.2.6`: GPL-2.0-or-later.
  - `rsigma 0.13.0`: MIT, but not a narrow parser replacement; EVTX support is
    part of a broader runtime feature set and requires newer Rust.

## Risk Position

This is an unmaintained dependency warning, not a known memory safety exploit in
the local adapter. The adapter is still risk-bounded:

- Parser input is capped by `MAX_EVTX_ANALYSIS_BYTES`.
- Results are advisory candidates with provenance and warnings.
- Malformed, truncated, and oversized inputs are tested to return warnings
  rather than fabricated records.
- A real `System.evtx` fixture now covers the parser path.

## Required Follow-Up Before Expiry

At least one of these must happen before `2026-09-01`:

1. Upgrade to a maintained `evtx` release that removes `encoding`.
2. Vendor or fork the minimal parser path and replace `encoding` with
   `encoding_rs` or a constrained Windows-1252 decode path.
3. Replace the adapter with another permissively licensed EVTX parser after
   targeted parser-path tests pass.
4. Re-review and renew the exception with a new owner, expiry, and technical
   justification.

Do not silently extend the exception without updating this decision record and
the development log.
