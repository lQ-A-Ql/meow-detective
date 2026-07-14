use std::collections::{HashMap, HashSet};

use super::wal_error;
use crate::{
    connection::{DbError, DbResult},
    repositories::{
        ceph_bluefs_replay_repo::CephBluefsFileRecord, ceph_bluefs_repo::CephBluefsAggregate,
    },
};

pub(super) fn expected_wal_root(bluefs: &CephBluefsAggregate) -> DbResult<&'static str> {
    if bluefs
        .replay
        .directories
        .iter()
        .any(|directory| directory.path == "db.wal")
    {
        return Ok("db.wal");
    }
    if bluefs
        .replay
        .directories
        .iter()
        .any(|directory| directory.path == "db")
    {
        return Ok("db");
    }
    wal_error("BlueFS replay contains neither db.wal nor legacy db directory")
}

pub(super) fn expected_wal_numbers(
    replay_files: &HashMap<&str, &CephBluefsFileRecord>,
    expected_root: &str,
    recovery_lower_bound: u64,
) -> DbResult<HashSet<u64>> {
    let prefix = format!("{expected_root}/");
    let mut numbers = HashSet::new();
    for file in replay_files.values() {
        let Some(file_name) = file.path.strip_prefix(&prefix) else {
            continue;
        };
        if file_name.contains('/') || file_name.contains('\\') {
            return wal_error("BlueFS WAL path is not a direct child of the selected root");
        }
        if !file_name.ends_with(".log") {
            continue;
        }
        let (root, wal_number) = parse_wal_path(&file.path)
            .ok_or_else(|| DbError::System("BlueFS WAL path is not canonical".to_string()))?;
        if root != expected_root
            || file.encoding != 0
            || (wal_number >= recovery_lower_bound && !numbers.insert(wal_number))
        {
            return wal_error("BlueFS selected WAL metadata is inconsistent");
        }
    }
    Ok(numbers)
}

pub(super) fn parse_wal_path(value: &str) -> Option<(&'static str, u64)> {
    let (root, digits) = if let Some(file_name) = value.strip_prefix("db.wal/") {
        ("db.wal", file_name.strip_suffix(".log")?)
    } else {
        ("db", value.strip_prefix("db/")?.strip_suffix(".log")?)
    };
    let number = (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse::<u64>().ok())
        .flatten()?;
    (number > 0 && value == format!("{root}/{number:06}.log")).then_some((root, number))
}
