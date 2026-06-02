# Tiny Fixtures

This directory contains small committed fixtures that are safe for default CI:

- `logical/` is a tiny logical directory tree for directory import and file path tests.
- `raw/tiny.raw` is a 1024-byte deterministic RAW image with an MBR signature.
- `e01/tiny.E01` is a deterministic synthetic single-segment E01 fixture for
  reader tests. It is not a full filesystem image.
- `evtx/system.evtx` is a small real System.evtx fixture copied from the
  MIT/Apache-2.0 licensed upstream `evtx` repository sample set at commit
  `38a2d50b21629edb3dd77953a2c02a4b944badf1`.
- `logical/Windows/System32/config/SYSTEM` and `SOFTWARE` are deterministic
  synthetic registry hives for Analysis targeted parser tests. They are not a
  complete Windows registry corpus.

Manual real-world E01 regression tests should use `FORENSICS_E01_FIXTURE` and
stay ignored or opt-in. The committed tiny E01 only proves bounded E01 reader
section/table/read/seek behavior.
