# Tiny Fixtures

This directory contains small committed fixtures that are safe for default CI:

- `logical/` is a tiny logical directory tree for directory import and file path tests.
- `raw/tiny.raw` is a 1024-byte deterministic RAW image with an MBR signature.

Tiny E01 fixtures are not committed yet. Manual E01 regression tests should use
`FORENSICS_E01_FIXTURE` and stay ignored or opt-in until a legal sub-1MB sample
is available.
