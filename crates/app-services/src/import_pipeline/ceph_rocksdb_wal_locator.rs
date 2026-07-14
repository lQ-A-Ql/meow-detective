use std::collections::HashSet;

use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use transport::CommandError;

use super::ceph_bluefs_replay::{BluefsReplayFile, BluefsReplaySnapshot};

pub(super) struct LocatedRocksdbWal<'a> {
    pub(super) wal_number: u64,
    pub(super) path: String,
    pub(super) post_manifest: bool,
    pub(super) file: &'a BluefsReplayFile,
}

pub(super) struct RocksdbWalSelection<'a> {
    pub(super) recovery_lower_bound: u64,
    pub(super) files: Vec<LocatedRocksdbWal<'a>>,
}

pub(super) fn locate_active_rocksdb_wals<'a>(
    replay: &'a BluefsReplaySnapshot,
    rocksdb: &CephRocksdbAggregate,
) -> Result<RocksdbWalSelection<'a>, CommandError> {
    let recovery_lower_bound = recovery_lower_bound(rocksdb)?;
    let wal_root = select_wal_root(replay)?;
    let mut seen_numbers = HashSet::new();
    let mut files = Vec::new();
    for file in &replay.files {
        let Some(wal_number) = parse_wal_path(&file.path, wal_root)? else {
            continue;
        };
        if !seen_numbers.insert(wal_number) {
            return Err(locator_error(format!(
                "RocksDB WAL file number {wal_number} is duplicated"
            )));
        }
        validate_wal_file(file)?;
        if wal_number >= recovery_lower_bound {
            files.push(LocatedRocksdbWal {
                wal_number,
                path: file.path.clone(),
                post_manifest: wal_number >= rocksdb.manifest.next_file_number,
                file,
            });
        }
    }
    files.sort_by_key(|file| file.wal_number);
    Ok(RocksdbWalSelection {
        recovery_lower_bound,
        files,
    })
}

fn recovery_lower_bound(rocksdb: &CephRocksdbAggregate) -> Result<u64, CommandError> {
    let minimum_unflushed = rocksdb
        .column_families
        .iter()
        .filter(|column_family| !column_family.dropped)
        .map(|column_family| column_family.log_number.unwrap_or_default())
        .min()
        .ok_or_else(|| locator_error("RocksDB has no active column family"))?;
    Ok(minimum_unflushed.max(rocksdb.manifest.min_log_number_to_keep.unwrap_or_default()))
}

fn select_wal_root(replay: &BluefsReplaySnapshot) -> Result<&'static str, CommandError> {
    if replay
        .directories
        .iter()
        .any(|directory| directory == "db.wal")
    {
        return Ok("db.wal");
    }
    if replay.directories.iter().any(|directory| directory == "db") {
        return Ok("db");
    }
    Err(locator_error(
        "BlueFS replay contains neither db.wal nor legacy db directory",
    ))
}

fn parse_wal_path(path: &str, wal_root: &str) -> Result<Option<u64>, CommandError> {
    let prefix = format!("{wal_root}/");
    let Some(file_name) = path.strip_prefix(&prefix) else {
        return Ok(None);
    };
    if file_name.contains('/') || file_name.contains('\\') {
        return Err(locator_error(format!(
            "RocksDB WAL path is not a direct {wal_root} child: {path}"
        )));
    }
    let Some(digits) = file_name.strip_suffix(".log") else {
        return Ok(None);
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(locator_error(format!(
            "RocksDB WAL path has a non-decimal file number: {path}"
        )));
    }
    let wal_number = digits
        .parse::<u64>()
        .map_err(|_| locator_error(format!("RocksDB WAL file number overflows u64: {path}")))?;
    if wal_number == 0 || path != format!("{wal_root}/{wal_number:06}.log") {
        return Err(locator_error(format!(
            "RocksDB WAL path is not canonical: {path}"
        )));
    }
    Ok(Some(wal_number))
}

fn validate_wal_file(file: &BluefsReplayFile) -> Result<(), CommandError> {
    if file.fnode.encoding != 0 {
        return Err(CommandError::unsupported(format!(
            "BlueFS RocksDB WAL {} uses unsupported content encoding {}",
            file.path, file.fnode.encoding
        )));
    }
    if file.fnode.size > 0 && file.fnode.extents.is_empty() {
        return Err(locator_error(format!(
            "BlueFS RocksDB WAL {} has no readable allocated content",
            file.path
        )));
    }
    Ok(())
}

fn locator_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB WAL location failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_wal_locator.rs"]
mod tests;
