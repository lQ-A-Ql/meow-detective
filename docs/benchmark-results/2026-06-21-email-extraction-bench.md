# Email Extraction Benchmark — 2026-06-21

## Environment

| Item | Value |
|---|---|
| Date | 2026-06-21 |
| Host | Windows (development workstation) |
| Toolchain | Rust stable (pinned by `rust-toolchain.toml`) |
| Build profile | `dev` / unoptimized |
| Test command | `cargo test -p containers-pst --test email_throughput_test -- --nocapture` |

## Datasets

| Dataset | Source | Size | Messages | Notes |
|---|---|---|---|---|
| `mbox-1MiB` | In-memory synthetic | 1.00 MiB | ~1,968 | Thunderbird-style `mboxrd` separators, plain-text messages |
| `pst-10msg` | `containers_pst::pst::build_synthetic_pst_with_messages(10)` | ~4 KiB | 10 | Unicode 64-bit synthetic PST |

## Results

| Scenario | Metric | Value | Threshold | Status |
|---|---|---|---|---|
| 1 MiB mbox parse | Wall time | 0.079 s | < 1.0 s | ✅ pass |
| 1 MiB mbox parse | Throughput | 12.64 MiB/s | — | — |
| 10-message synthetic PST parse | Wall time | 5.21 ms | < 100 ms | ✅ pass |
| 10-message synthetic PST parse | Throughput | ~1,920 messages/s | — | — |

## Boundary Notes

- The current mbox parser comfortably beats the V2 acceptance threshold of
  **1 MiB in < 1 second**.
- The current PST reader is fixture-small because the synthetic builder is
  intentionally limited to single-page NBT/BBT structures. A 10 MiB PST
  benchmark is deferred until a block-cached / streaming PST reader is
  implemented; the existing threshold of **10 MiB in < 10 seconds** is
  therefore recorded as a planned release gate, not a measured one.
- Very large PST/OST files are currently loaded entirely into memory during
  extraction. The analysis pipeline records a warning when a source exceeds
  `MAX_ANALYSIS_SOURCE_BYTES`, and encrypted PST/OST files are rejected with
  a descriptive error.

## CI Placement

- This benchmark runs as part of the `containers-pst` crate test suite.
- It is a `small` / PR-level benchmark.
- A nightly `medium` benchmark using the `public-medium/email/` fixtures is
  planned once the synthetic PST builder supports larger multi-page fixtures.

## References

- `docs/benchmark-baseline.md`
- `docs/pst-ost-mbox-support.md`
- `crates/containers-pst/tests/email_throughput_test.rs`
