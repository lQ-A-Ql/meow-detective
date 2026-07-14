use std::collections::HashMap;

use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
    CephRocksdbManifestRecord,
};
use transport::CommandError;

use super::ceph_bluefs_replay::{BluefsReplayFile, BluefsReplaySnapshot};

pub(super) struct LocatedRocksdbSst<'a> {
    pub(super) path: String,
    pub(super) file: &'a BluefsReplayFile,
    pub(super) live: &'a CephRocksdbLiveSstRecord,
    pub(super) column_family: &'a CephRocksdbColumnFamilyRecord,
    pub(super) manifest: &'a CephRocksdbManifestRecord,
}

pub(super) fn locate_live_rocksdb_ssts<'a>(
    replay: &'a BluefsReplaySnapshot,
    rocksdb: &'a CephRocksdbAggregate,
) -> Result<Vec<LocatedRocksdbSst<'a>>, CommandError> {
    let files_by_path = index_replay_files(replay)?;
    let column_families = index_column_families(rocksdb)?;
    let mut located = rocksdb
        .live_ssts
        .iter()
        .map(|live| locate_live_sst(&files_by_path, &column_families, rocksdb, live))
        .collect::<Result<Vec<_>, _>>()?;
    located.sort_by_key(|sst| {
        (
            sst.live.column_family_id,
            sst.live.level,
            sst.live.file_number,
        )
    });
    Ok(located)
}

fn locate_live_sst<'a>(
    files_by_path: &HashMap<&str, &'a BluefsReplayFile>,
    column_families: &HashMap<u32, &'a CephRocksdbColumnFamilyRecord>,
    rocksdb: &'a CephRocksdbAggregate,
    live: &'a CephRocksdbLiveSstRecord,
) -> Result<LocatedRocksdbSst<'a>, CommandError> {
    if live.path_id != 0 {
        return Err(CommandError::unsupported(format!(
            "RocksDB live SST {} uses unsupported path ID {}",
            live.file_number, live.path_id
        )));
    }
    let path = live_sst_path(live.file_number)?;
    let file = files_by_path
        .get(path.as_str())
        .copied()
        .ok_or_else(|| locator_error(format!("BlueFS replay is missing live SST {path}")))?;
    validate_file_identity(file, live)?;
    let column_family = column_families
        .get(&live.column_family_id)
        .copied()
        .filter(|record| !record.dropped)
        .ok_or_else(|| {
            locator_error(format!(
                "live SST {} references missing or dropped column family {}",
                live.file_number, live.column_family_id
            ))
        })?;
    Ok(LocatedRocksdbSst {
        path,
        file,
        live,
        column_family,
        manifest: &rocksdb.manifest,
    })
}

fn live_sst_path(file_number: u64) -> Result<String, CommandError> {
    if file_number == 0 {
        return Err(CommandError::unsupported(format!(
            "RocksDB live SST file number {file_number} is not a valid RocksDB file number"
        )));
    }
    Ok(format!("db/{file_number:06}.sst"))
}

fn index_replay_files(
    replay: &BluefsReplaySnapshot,
) -> Result<HashMap<&str, &BluefsReplayFile>, CommandError> {
    let mut files = HashMap::with_capacity(replay.files.len());
    for file in &replay.files {
        if files.insert(file.path.as_str(), file).is_some() {
            return Err(locator_error(format!(
                "BlueFS replay contains duplicate file path {}",
                file.path
            )));
        }
    }
    Ok(files)
}

fn index_column_families(
    rocksdb: &CephRocksdbAggregate,
) -> Result<HashMap<u32, &CephRocksdbColumnFamilyRecord>, CommandError> {
    let mut column_families = HashMap::with_capacity(rocksdb.column_families.len());
    for column_family in &rocksdb.column_families {
        if column_families
            .insert(column_family.column_family_id, column_family)
            .is_some()
        {
            return Err(locator_error(format!(
                "RocksDB replay contains duplicate column family {}",
                column_family.column_family_id
            )));
        }
    }
    Ok(column_families)
}

fn validate_file_identity(
    file: &BluefsReplayFile,
    live: &CephRocksdbLiveSstRecord,
) -> Result<(), CommandError> {
    if file.fnode.encoding != 0 {
        return Err(CommandError::unsupported(format!(
            "BlueFS live SST {} uses unsupported content encoding {}",
            file.path, file.fnode.encoding
        )));
    }
    if file.fnode.size != live.file_size {
        return Err(locator_error(format!(
            "BlueFS live SST {} size {} does not match MANIFEST size {}",
            file.path, file.fnode.size, live.file_size
        )));
    }
    if file.fnode.size == 0 || file.fnode.extents.is_empty() {
        return Err(locator_error(format!(
            "BlueFS live SST {} has no readable allocated content",
            file.path
        )));
    }
    Ok(())
}

fn locator_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB live-SST location failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_sst_locator.rs"]
mod tests;
