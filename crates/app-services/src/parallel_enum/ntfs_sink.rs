use super::batch_sink::{extension_from_name, visibility_flags_for_name};
use fs_ntfs::mft_scanner::MftRecord;
use rusqlite::{CachedStatement, Connection};

pub(super) fn prepare_mft_insert(conn: &Connection) -> rusqlite::Result<CachedStatement<'_>> {
    conn.prepare_cached(
        "INSERT OR IGNORE INTO ntfs_entries
         (record_num, parent_record_num, sequence_num, name, entry_type, size, ext,
          deleted, hidden, system, read_only, archive, encrypted, created_at, modified_at, accessed_at, changed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)"
    )
}

pub(super) fn stage_mft_record(
    statement: &mut CachedStatement<'_>,
    record: &MftRecord,
    _data_source_id: &str,
    _partition_index: usize,
) -> rusqlite::Result<(Option<u64>, String, u64)> {
    let name = if record.record_number == 5 && (record.name.is_empty() || record.name == ".") {
        "\\".to_string()
    } else {
        record.name.clone()
    };
    let parent_key = (record.record_number != 5).then_some(record.parent_ref);
    let (hidden, system) = visibility_flags_for_name(&name, record.hidden, record.system);
    let inserted = statement.execute(rusqlite::params![
        record.record_number,
        parent_key,
        record.sequence_number,
        name,
        record.is_dir as i32,
        (!record.is_dir).then_some(record.size),
        (!record.is_dir)
            .then(|| extension_from_name(&record.name))
            .flatten(),
        record.deleted as i32,
        hidden as i32,
        system as i32,
        record.read_only as i32,
        record.archive as i32,
        record.encrypted as i32,
        record.created_at.as_ref().map(|value| value.to_rfc3339()),
        record.modified_at.as_ref().map(|value| value.to_rfc3339()),
        record.accessed_at.as_ref().map(|value| value.to_rfc3339()),
        record.changed_at.as_ref().map(|value| value.to_rfc3339()),
    ])?;
    Ok((parent_key, name, inserted as u64))
}

pub(super) fn prepare_ntfs_index_insert(
    conn: &Connection,
) -> rusqlite::Result<CachedStatement<'_>> {
    conn.prepare_cached(
        "INSERT OR IGNORE INTO ntfs_entries
         (record_num, parent_record_num, sequence_num, name, entry_type, size, ext,
          deleted, hidden, system, read_only, archive, encrypted)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?11)",
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_ntfs_index_entry(
    statement: &mut CachedStatement<'_>,
    _data_source_id: &str,
    _partition_index: usize,
    parent_ref: u64,
    mft_ref: u64,
    name: &str,
    is_dir: bool,
    size: u64,
    hidden: bool,
    system: bool,
    read_only: bool,
    archive: bool,
    encrypted: bool,
) -> rusqlite::Result<usize> {
    let (hidden, system) = visibility_flags_for_name(name, hidden, system);
    statement.execute(rusqlite::params![
        mft_ref,
        parent_ref,
        name,
        is_dir as i32,
        (!is_dir).then_some(size),
        (!is_dir).then(|| extension_from_name(name)).flatten(),
        hidden as i32,
        system as i32,
        read_only as i32,
        archive as i32,
        encrypted as i32,
    ])
}
