//! systemd Journal binary format parser.
//!
//! Parse systemd's binary journal files (e.g. /var/log/journal/<machine-id>/*.journal).
//! Supports both uncompressed (DATA_OBJECT) and LZ4/ZSTD-compressed fields.
//!
//! Format reference: https://systemd.io/JOURNAL_FILE_FORMAT/

mod entry;
mod header;
mod object;
mod projection;

pub use entry::JournalEntry;
pub use projection::parse_journal;
