# Platform and checksum dependency integration

Source: crates.io `bbolt-rs 1.3.10`, upstream commit
`d51d0bc2f2f01a229187d48eb7e8440c127cf917`.

The local patch gates Unix-only mmap advice and locking APIs and the
platform-specific timeout/mlock option fields with their declared cfg aliases.
Bolt page decoding uses the upstream implementation. The FNV-1a checksum
provider is `fnv 1.0.7`, replacing `fnv_rs` and its unmaintained `paste`
dependency. The checksum input and algorithm remain unchanged; Go bbolt
fixtures verify both metadata pages and checksum rejection.

Upstream: https://github.com/ambaxter/bbolt-rs
License: MIT, retained in LICENSE.
