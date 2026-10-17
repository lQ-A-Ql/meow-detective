use super::super::batch_sink::{insert_ntfs_index_entry, stage_mft_record, EnumerationStats};
use super::super::error::ParallelEnumError;
use super::size_reconciliation::ExternalFileSizes;
use fs_ntfs::mft_scanner::MftRecord;
use fs_ntfs::NtfsDirectoryEntry;
use rusqlite::{CachedStatement, Connection};
use std::collections::{HashMap, HashSet};

pub(in crate::parallel_enum) type PathMap = HashMap<u64, (Option<u64>, String, bool)>;

#[derive(Default)]
pub(in crate::parallel_enum) struct MftCatalog {
    path_map: PathMap,
    deleted_records: HashSet<u64>,
    record_sequences: HashMap<u64, u16>,
    index_representatives: HashMap<u64, (u64, String, bool)>,
    external_file_sizes: ExternalFileSizes,
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
    ) -> Result<u64, ParallelEnumError> {
        let mut staged_rows = 0u64;
        for record in records.iter().filter(|record| {
            record.is_valid && (!record.name.is_empty() || record.record_number == 5)
        }) {
            self.external_file_sizes.track(record);
            self.record_sequences
                .insert(record.record_number, record.sequence_number);
            let (parent_key, name, inserted) = stage_mft_record(statement, record)?;
            staged_rows += inserted;
            self.path_map
                .insert(record.record_number, (parent_key, name, record.is_dir));
            if record.deleted {
                self.deleted_records.insert(record.record_number);
            }
            if record.is_dir {
                self.stats.dir_count += 1;
            } else {
                self.stats.file_count += 1;
                self.stats.total_size += record.size;
            }
        }
        Ok(staged_rows)
    }

    pub(in crate::parallel_enum) fn update_staging_paths(
        &self,
        conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<super::numeric_paths::PathMaterializationStats, ParallelEnumError> {
        update_mft_staging_paths_via_sqlite(
            conn,
            data_source_id,
            partition_index,
            &self.path_map,
            &self.deleted_records,
            cancel,
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
            action.read_only,
            action.archive,
            action.encrypted,
        )?;
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
    pub(in crate::parallel_enum) read_only: bool,
    pub(in crate::parallel_enum) archive: bool,
    pub(in crate::parallel_enum) encrypted: bool,
}

pub(in crate::parallel_enum) fn mft_directory_index_backfill_actions_with_representatives(
    path_map: &mut PathMap,
    representatives: &mut HashMap<u64, (u64, String, bool)>,
    directory_ref: u64,
    mut entries: Vec<NtfsDirectoryEntry>,
) -> Vec<MftDirectoryIndexBackfillAction> {
    // fs-ntfs orders aliases by namespace rank. Stable sorting by record keeps
    // the preferred modern name first for the inode-keyed catalog insert.
    entries.sort_by_key(|entry| entry.mft_ref);
    let parent_key = directory_ref;
    let mut actions = Vec::new();
    let mut selected_records = HashSet::new();
    for entry in entries {
        if entry.name.is_empty() || entry.mft_ref == directory_ref {
            continue;
        }
        let record_key = entry.mft_ref;
        if selected_records.insert(record_key) {
            select_index_representative(
                path_map,
                representatives,
                record_key,
                directory_ref,
                &parent_key,
                &entry,
            );
        }
        actions.push(MftDirectoryIndexBackfillAction {
            name: entry.name,
            is_dir: entry.is_dir,
            size: entry.size,
            mft_ref: entry.mft_ref,
            hidden: entry.hidden,
            system: entry.system,
            read_only: entry.read_only,
            archive: entry.archive,
            encrypted: entry.encrypted,
        });
    }
    actions
}

pub(in crate::parallel_enum) fn directory_reference_sequence_matches(
    record_sequences: &HashMap<u64, u16>,
    record_number: u64,
    reference_sequence: u16,
) -> bool {
    record_sequences
        .get(&record_number)
        .is_none_or(|expected| *expected == reference_sequence)
}

fn select_index_representative(
    path_map: &mut PathMap,
    representatives: &mut HashMap<u64, (u64, String, bool)>,
    record_key: u64,
    parent_ref: u64,
    parent_key: &u64,
    entry: &NtfsDirectoryEntry,
) {
    let candidate = (parent_ref, entry.name.clone(), entry.is_dir);
    let should_update = representatives
        .get(&record_key)
        .is_none_or(|current| candidate < *current);
    if should_update {
        representatives.insert(record_key, candidate);
        path_map.insert(
            record_key,
            (Some(*parent_key), entry.name.clone(), entry.is_dir),
        );
    }
}

pub(in crate::parallel_enum) use super::numeric_paths::update_mft_staging_paths_via_sqlite;

mod directory_backfill;
