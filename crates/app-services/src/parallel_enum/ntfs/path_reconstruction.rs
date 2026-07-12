use super::super::batch_sink::{
    insert_ntfs_index_entry, mft_entry_id, prepare_ntfs_index_insert, stage_mft_record,
    EnumerationStats,
};
use super::super::error::ParallelEnumError;
use super::super::partition_work::PartitionWork;
use evidence_core::EvidenceReader;
use fs_ntfs::mft_scanner::MftRecord;
use fs_ntfs::{NtfsDirectoryEntry, NtfsReader};
use rusqlite::{params, CachedStatement, Connection};
use std::collections::{HashMap, HashSet, VecDeque};

pub(in crate::parallel_enum) type PathMap = HashMap<String, (Option<String>, String, bool)>;

#[derive(Default)]
pub(in crate::parallel_enum) struct MftCatalog {
    path_map: PathMap,
    deleted_records: HashSet<String>,
    stats: EnumerationStats,
}

impl MftCatalog {
    pub(in crate::parallel_enum) fn stats(&self) -> EnumerationStats {
        self.stats
    }

    pub(in crate::parallel_enum) fn stage_records(
        &mut self,
        statement: &mut CachedStatement<'_>,
        records: &[MftRecord],
        data_source_id: &str,
        partition_index: usize,
    ) -> Result<(), String> {
        for record in records.iter().filter(|record| {
            record.is_valid && (!record.name.is_empty() || record.record_number == 5)
        }) {
            let (parent_key, name) =
                stage_mft_record(statement, record, data_source_id, partition_index)?;
            self.path_map.insert(
                record.record_number.to_string(),
                (parent_key, name, record.is_dir),
            );
            if record.deleted {
                self.deleted_records
                    .insert(record.record_number.to_string());
            }
            if record.is_dir {
                self.stats.dir_count += 1;
            } else {
                self.stats.file_count += 1;
                self.stats.total_size += record.size;
            }
        }
        Ok(())
    }

    pub(in crate::parallel_enum) fn backfill_directory_indexes(
        &mut self,
        conn: &Connection,
        data_source_id: &str,
        evidence_reader: Box<dyn EvidenceReader>,
        partition: &PartitionWork,
    ) -> Result<(), ParallelEnumError> {
        let ntfs = NtfsReader::open(evidence_reader, partition.volume_offset).map_err(|error| {
            ParallelEnumError::MftParams(format!("Open NTFS reader for directory indexes: {error}"))
        })?;
        let mut statement =
            prepare_ntfs_index_insert(conn).map_err(ParallelEnumError::MftParams)?;
        let mut queue = VecDeque::from([5]);
        let mut visited = HashSet::new();

        while let Some(directory_ref) = queue.pop_front() {
            if !visited.insert(directory_ref) {
                continue;
            }
            let entries = match ntfs.list_directory_entries_by_inode(directory_ref) {
                Ok(entries) => entries,
                Err(error) => {
                    tracing::warn!(
                        "Failed to list NTFS directory index {}: {}",
                        directory_ref,
                        error
                    );
                    continue;
                }
            };
            let actions =
                mft_directory_index_backfill_actions(&mut self.path_map, directory_ref, entries);
            for action in actions {
                self.insert_backfill_action(
                    &mut statement,
                    data_source_id,
                    partition.index,
                    directory_ref,
                    &action,
                )?;
                if action.is_dir && !visited.contains(&action.mft_ref) {
                    queue.push_back(action.mft_ref);
                }
            }
        }
        Ok(())
    }

    pub(in crate::parallel_enum) fn update_staging_paths(
        &self,
        conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
    ) -> rusqlite::Result<()> {
        update_mft_staging_paths_via_sqlite(
            conn,
            data_source_id,
            partition_index,
            &self.path_map,
            &self.deleted_records,
        )
    }

    fn insert_backfill_action(
        &mut self,
        statement: &mut CachedStatement<'_>,
        data_source_id: &str,
        partition_index: usize,
        parent_ref: u64,
        action: &MftDirectoryIndexBackfillAction,
    ) -> Result<(), ParallelEnumError> {
        let changed = insert_ntfs_index_entry(
            statement,
            data_source_id,
            partition_index,
            parent_ref,
            action.mft_ref,
            &action.name,
            action.is_dir,
            action.size,
            action.hidden,
            action.system,
        )
        .map_err(ParallelEnumError::MftParams)?;
        if changed > 0 {
            if action.is_dir {
                self.stats.dir_count += 1;
            } else {
                self.stats.file_count += 1;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::parallel_enum) struct MftDirectoryIndexBackfillAction {
    pub(in crate::parallel_enum) name: String,
    pub(in crate::parallel_enum) is_dir: bool,
    pub(in crate::parallel_enum) size: u64,
    pub(in crate::parallel_enum) mft_ref: u64,
    pub(in crate::parallel_enum) hidden: bool,
    pub(in crate::parallel_enum) system: bool,
}

pub(in crate::parallel_enum) fn mft_directory_index_backfill_actions(
    path_map: &mut PathMap,
    directory_ref: u64,
    mut entries: Vec<NtfsDirectoryEntry>,
) -> Vec<MftDirectoryIndexBackfillAction> {
    entries.sort_by(|left, right| {
        left.mft_ref
            .cmp(&right.mft_ref)
            .then_with(|| left.name.cmp(&right.name))
    });
    let parent_key = directory_ref.to_string();
    let mut actions = Vec::new();
    for entry in entries {
        if entry.name.is_empty() || entry.mft_ref == directory_ref {
            continue;
        }
        let record_key = entry.mft_ref.to_string();
        if index_entry_should_update(path_map, &record_key, &parent_key, &entry) {
            path_map.insert(
                record_key,
                (Some(parent_key.clone()), entry.name.clone(), entry.is_dir),
            );
        }
        actions.push(MftDirectoryIndexBackfillAction {
            name: entry.name,
            is_dir: entry.is_dir,
            size: entry.size,
            mft_ref: entry.mft_ref,
            hidden: entry.hidden,
            system: entry.system,
        });
    }
    actions
}

fn index_entry_should_update(
    path_map: &PathMap,
    record_key: &str,
    parent_key: &str,
    entry: &NtfsDirectoryEntry,
) -> bool {
    match path_map.get(record_key) {
        None => true,
        Some((parent, name, is_dir)) => {
            name.is_empty()
                || parent.is_none()
                || parent.as_deref() == Some(record_key)
                || parent
                    .as_deref()
                    .is_some_and(|value| !path_map.contains_key(value))
                || (parent.as_deref() == Some("5") && parent_key != "5")
                || parent.as_deref() != Some(parent_key)
                || *is_dir != entry.is_dir
        }
    }
}

pub(in crate::parallel_enum) fn validate_mft_staging_shape(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
) -> Result<(), ParallelEnumError> {
    let root_id = mft_entry_id(partition_index, 5);
    let system32 = root_child_count(conn, data_source_id, &root_id, "System32")?;
    let hives = root_child_count(conn, data_source_id, &root_id, "SOFTWARE")?
        + root_child_count(conn, data_source_id, &root_id, "System.evtx")?;
    let windows = directory_name_count(conn, data_source_id, partition_index, "Windows")?;
    let users = directory_name_count(conn, data_source_id, partition_index, "Users")?;
    if windows == 0 && users == 0 && (system32 > 0 || hives > 0) {
        return Err(ParallelEnumError::MftParams(format!(
            "MFT fast path produced suspicious flat NTFS tree: root System32={system32}, root hive/log candidates={hives}, Windows dirs={windows}, Users dirs={users}. Falling back to recursive NTFS reader."
        )));
    }
    Ok(())
}

fn root_child_count(
    conn: &Connection,
    data_source_id: &str,
    root_id: &str,
    name: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND parent_id = ?2 AND name = ?3 COLLATE NOCASE",
        params![data_source_id, root_id, name],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn directory_name_count(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
    name: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND id LIKE ?2
           AND entry_type = 'directory' COLLATE NOCASE AND name = ?3 COLLATE NOCASE",
        params![data_source_id, format!("mft:{partition_index}:%"), name],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

pub(in crate::parallel_enum) fn update_mft_staging_paths_via_sqlite(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
    path_map: &PathMap,
    deleted_records: &HashSet<String>,
) -> rusqlite::Result<()> {
    initialize_path_table(conn)?;
    insert_path_records(conn, path_map)?;
    let resolved = resolve_all_paths(path_map, deleted_records, partition_index);
    write_resolved_paths(conn, &resolved)?;
    drop(resolved);
    update_file_paths(conn, data_source_id, partition_index)?;
    update_parent_ids(conn, data_source_id, partition_index)
}

fn initialize_path_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS mft_path_records (
             record_num TEXT PRIMARY KEY,
             parent_num TEXT,
             name TEXT NOT NULL,
             is_dir INTEGER NOT NULL,
             resolved_path TEXT
         );
         DELETE FROM mft_path_records;",
    )
}

fn insert_path_records(conn: &Connection, path_map: &PathMap) -> rusqlite::Result<()> {
    let mut statement = conn.prepare_cached(
        "INSERT OR REPLACE INTO mft_path_records
         (record_num, parent_num, name, is_dir) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut records = path_map.iter().collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(right.0));
    for (record, (parent, name, is_dir)) in records {
        statement.execute(params![record, parent, name, *is_dir as i32])?;
    }
    Ok(())
}

fn resolve_all_paths(
    path_map: &PathMap,
    deleted_records: &HashSet<String>,
    partition_index: usize,
) -> HashMap<String, String> {
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut records = path_map.keys().collect::<Vec<_>>();
    records.sort();
    for record in records {
        resolve_mft_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            partition_index,
        );
    }
    resolved
}

fn write_resolved_paths(
    conn: &Connection,
    resolved: &HashMap<String, String>,
) -> rusqlite::Result<()> {
    let mut statement = conn
        .prepare_cached("UPDATE mft_path_records SET resolved_path = ?1 WHERE record_num = ?2")?;
    let mut records = resolved.iter().collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(right.0));
    for (record, path) in records {
        statement.execute(params![path, record])?;
    }
    Ok(())
}

fn update_file_paths(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
) -> rusqlite::Result<()> {
    let prefix = format!("mft:{partition_index}:");
    conn.execute(
        "UPDATE file_entries SET path = (
             SELECT resolved_path FROM mft_path_records
             WHERE record_num = substr(file_entries.id, ?1)
         )
         WHERE data_source_id = ?2 AND id LIKE ?3
           AND EXISTS (
             SELECT 1 FROM mft_path_records
             WHERE record_num = substr(file_entries.id, ?1)
           )",
        params![prefix.len() + 1, data_source_id, format!("{prefix}%")],
    )?;
    Ok(())
}

fn update_parent_ids(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
) -> rusqlite::Result<()> {
    let prefix = format!("mft:{partition_index}:");
    conn.execute(
        "UPDATE file_entries SET parent_id = CASE
             WHEN substr(file_entries.id, ?1) = '5' THEN NULL
             WHEN EXISTS (
                 SELECT 1 FROM mft_path_records parent
                 WHERE parent.record_num = (
                     SELECT child.parent_num FROM mft_path_records child
                     WHERE child.record_num = substr(file_entries.id, ?1)
                 )
             ) THEN ?4 || (
                 SELECT child.parent_num FROM mft_path_records child
                 WHERE child.record_num = substr(file_entries.id, ?1)
             )
             WHEN EXISTS (SELECT 1 FROM mft_path_records WHERE record_num = '5')
                 THEN ?4 || '5'
             ELSE NULL
         END
         WHERE data_source_id = ?2 AND id LIKE ?3",
        params![
            prefix.len() + 1,
            data_source_id,
            format!("{prefix}%"),
            prefix
        ],
    )?;
    Ok(())
}

fn resolve_mft_path(
    record: &str,
    path_map: &PathMap,
    deleted_records: &HashSet<String>,
    resolved: &mut HashMap<String, String>,
    partition_index: usize,
) -> String {
    if let Some(path) = resolved.get(record) {
        return path.clone();
    }
    let mut chain = Vec::new();
    let mut current = record.to_string();
    let mut seen = HashSet::from([current.clone()]);
    let base = loop {
        if let Some(path) = resolved.get(&current) {
            break path.clone();
        }
        match path_map.get(&current) {
            Some((Some(parent), name, _))
                if parent != &current
                    && path_map.contains_key(parent)
                    && seen.insert(parent.clone()) =>
            {
                chain.push((current.clone(), name.clone()));
                current = parent.clone();
            }
            Some((_, name, _)) if current == "5" => {
                resolved.insert(current.clone(), String::new());
                break String::new();
            }
            Some((_, name, _)) if deleted_records.contains(&current) => {
                let path = format!("/$DeletedOrphans/{current}-{name}");
                resolved.insert(current.clone(), path.clone());
                break path;
            }
            Some((_, name, _)) => {
                let path = format!("/Unresolved/{name}");
                resolved.insert(current.clone(), path.clone());
                break path;
            }
            None => {
                let path = format!("/Unresolved/{current}");
                resolved.insert(current.clone(), path.clone());
                break path;
            }
        }
    };
    let mut path = base;
    for (id, name) in chain.into_iter().rev() {
        path = if path.is_empty() {
            name
        } else {
            format!("{path}/{name}")
        };
        resolved.insert(id, path.clone());
    }
    let path = resolved.get(record).cloned().unwrap_or(path);
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        format!("[P{partition_index}]")
    } else {
        format!("[P{partition_index}]/{trimmed}")
    }
}
