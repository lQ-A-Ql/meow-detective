use std::io::Cursor;

use crate::LinuxArtifactError;

use super::entry::{parse_entry_at_offset, JournalEntry};
use super::header::{Header, HEADER_INCOMPATIBLE_COMPRESSED};
use super::object::collect_entry_offsets;

pub fn parse_journal(data: &[u8]) -> Result<Vec<JournalEntry>, LinuxArtifactError> {
    if data.len() < 240 {
        return Err(LinuxArtifactError::ParseError {
            parser: "journal",
            message: "Data too short to be a systemd journal file".to_string(),
        });
    }

    let mut reader = Cursor::new(data);
    let header = Header::read(&mut reader)?;
    let has_compressed = (header.incompatible_flags() & HEADER_INCOMPATIBLE_COMPRESSED) != 0;
    let entry_offsets = collect_entry_offsets(&mut reader, data, &header);

    let mut entries = Vec::new();
    for entry_offset in entry_offsets {
        if let Some(entry) = parse_entry_at_offset(&mut reader, data, entry_offset, has_compressed)
        {
            entries.push(entry);
        }
    }

    Ok(entries)
}
