//! systemd journal binary format parser.
//!
//! Parses systemd journal files (e.g. `/var/log/journal/<machine-id>/*.journal`)
//! following the on-disk format documented at
//! <https://systemd.io/JOURNAL_FILE_FORMAT/> and implemented by systemd's
//! `src/libsystemd/sd-journal/journal-def.h`, `journal-file.c` and
//! `src/basic/compress.c`.
//!
//! Supported:
//! - Regular object layout (pre-v254) and the COMPACT layout
//!   (`HEADER_INCOMPATIBLE_COMPACT`, 32-bit object offsets).
//! - Unkeyed Jenkins lookup3 hashes (pre-v246 files) and keyed SipHash-2-4
//!   hashes (`HEADER_INCOMPATIBLE_KEYED_HASH`, keyed by the header `file_id`).
//! - LZ4 and Zstd compressed DATA payloads (`OBJECT_COMPRESSED_LZ4` /
//!   `OBJECT_COMPRESSED_ZSTD`).
//! - Truncated tails, e.g. `STATE_ONLINE` files imaged while still open for
//!   writing; the truncation is reported, not treated as an error.
//!
//! Not supported:
//! - XZ-compressed DATA payloads: detected via `OBJECT_COMPRESSED_XZ`, skipped
//!   and counted in `JournalParseOutcome::skipped_compressed`.
//! - Files carrying unknown `incompatible_flags` bits are rejected with
//!   `LinuxArtifactError::Unsupported`, as required by the format spec.
//! - Forward Secure Sealing: TAG objects are skipped without verification.
//!
//! Field identification reads each DATA object payload (`FIELD=value`) and
//! splits at the first `=`. Entry item hashes / DATA hashes are verified
//! against the payload and mismatches are counted, but payloads are still
//! used, matching the "handle corruption gracefully" rule of the spec.

mod compress;
mod entry;
pub mod hash;
mod header;
mod object;
mod projection;

pub use entry::JournalEntry;
pub use projection::{parse_journal, parse_journal_full, JournalParseOutcome};
