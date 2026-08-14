use crate::{connection::DbResult, util::parse_opt_datetime};
use domain::{DataSourceId, EntryType, FileEncryptionStatus, FileEntry, FileEntryId};

pub(super) fn escape_like_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

pub(super) fn row_to_file_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileEntry> {
    let entry_type: String = row.get(5)?;
    let encryption_status = file_encryption_status_from_row(row, 16)?;
    Ok(FileEntry {
        id: FileEntryId(row.get::<_, String>(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(FileEntryId),
        data_source_id: DataSourceId(row.get::<_, String>(2)?),
        path: row.get(3)?,
        name: row.get(4)?,
        entry_type: if entry_type.eq_ignore_ascii_case("directory") {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: row.get(6)?,
        ext: row.get(7)?,
        deleted: row.get::<_, i32>(8)? != 0,
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
        encrypted: encryption_status.blocks_content(),
        created_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|value| parse_opt_datetime(&value)),
        modified_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|value| parse_opt_datetime(&value)),
        accessed_at: row
            .get::<_, Option<String>>(13)?
            .and_then(|value| parse_opt_datetime(&value)),
        changed_at: row
            .get::<_, Option<String>>(14)?
            .and_then(|value| parse_opt_datetime(&value)),
        hash_sha256: row.get(15)?,
        read_only: row.get::<_, i32>(17)? != 0,
        archive: row.get::<_, i32>(18)? != 0,
    })
}

pub fn file_encryption_status_from_row(
    row: &rusqlite::Row<'_>,
    column_index: usize,
) -> rusqlite::Result<FileEncryptionStatus> {
    let value = row.get::<_, Option<i64>>(column_index)?;
    FileEncryptionStatus::from_database_value(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

pub(super) fn collect_entries(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<FileEntry>>,
) -> DbResult<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}
