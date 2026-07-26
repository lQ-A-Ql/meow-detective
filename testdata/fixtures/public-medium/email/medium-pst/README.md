# public-medium PST/OST fixtures

Synthetic Unicode PST/OST files containing 10 email messages each.

## Samples

| File | Type | Purpose |
|------|------|---------|
| `medium.pst` | PST | 10 synthetic messages with subject/body/sender |
| `medium.ost` | OST | Same content as `medium.pst`, renamed extension |

## Visibility

public-medium

## Provenance

- Generator: `cargo run -p containers-pst --example generate_medium_fixture` (`crates/containers-pst/examples/generate_medium_fixture.rs`).
- Reproducibility: both files are generated from the same deterministic ten-message NDB/LTP builder.
- License: repository MIT license.
- Sensitivity review: synthetic sender, subject, and body values only; no personal data, credentials, tokens, or workstation paths.

| File | Bytes | SHA-256 |
|------|------:|---------|
| `medium.pst` | 10240 | `96ec8950c174d1e93fe40da0ba803e22680e11641d2ab4d7cf291cebd09d12aa` |
| `medium.ost` | 10240 | `96ec8950c174d1e93fe40da0ba803e22680e11641d2ab4d7cf291cebd09d12aa` |

## Expected JSON

`expected.json` in this directory.
