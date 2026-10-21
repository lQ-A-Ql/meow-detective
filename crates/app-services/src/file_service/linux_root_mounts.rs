//! Resolve configured Linux root mounts from source-scoped fstab evidence.
use domain::{DataSourceId, EntryType, FileEntryId};
use persistence_sqlite::repositories::{file_repo::FileRepo, partition_repo::PartitionRepo};
use rusqlite::Connection;
use transport::dto::{DataSourcePartitionDto, LinuxRootMountDto};

use super::FileServiceError;

const MAX_FSTAB_BYTES: usize = 64 * 1024;
const MAX_PARTITIONS: usize = 64;

pub(super) fn collect(
    conn: &Connection,
    source: &DataSourceId,
    partitions: &[DataSourcePartitionDto],
    mut read: impl FnMut(&FileEntryId, u32) -> Result<Vec<u8>, FileServiceError>,
) -> Result<Vec<LinuxRootMountDto>, FileServiceError> {
    let repo = FileRepo::new(conn);
    let records = PartitionRepo::new(conn).find_by_data_source(&source.0)?;
    let mut results = Vec::new();
    for partition in partitions
        .iter()
        .filter(|item| matches!(item.filesystem.as_deref(), Some("Ext4" | "XFS" | "Btrfs")))
        .take(MAX_PARTITIONS)
    {
        let Some(entry) =
            repo.find_by_partition_and_path(source, partition.index as usize, "etc/fstab")?
        else {
            continue;
        };
        if entry.entry_type != EntryType::File
            || entry.size.is_some_and(|n| n > MAX_FSTAB_BYTES as u64)
        {
            continue;
        }
        let Ok(bytes) = read(&entry.id, MAX_FSTAB_BYTES as u32 + 1) else {
            continue;
        };
        if bytes.len() > MAX_FSTAB_BYTES || entry.size.is_some_and(|n| n != bytes.len() as u64) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let entries = root_entries(text);
        let multiple = entries.len() > 1;
        for (line, spec, filesystem) in entries {
            let matches: Vec<_> = partitions
                .iter()
                .filter(|target| {
                    let record = records
                        .iter()
                        .find(|record| record.partition_index == target.index);
                    matches_spec(&spec, target, record)
                })
                .collect();
            let resolved = (!multiple && matches.len() == 1)
                .then(|| matches[0])
                .filter(|target| {
                    filesystem == "auto"
                        || target
                            .filesystem
                            .as_deref()
                            .is_some_and(|kind| kind.eq_ignore_ascii_case(&filesystem))
                });
            results.push(LinuxRootMountDto {
                fstab_partition_index: partition.index,
                source_spec: spec,
                evidence_path: entry.path.clone(),
                evidence_line: line,
                status: if multiple || matches.len() > 1 {
                    "ambiguous"
                } else if resolved.is_some() {
                    "confirmed"
                } else {
                    "unresolved"
                }
                .into(),
                resolved_partition_index: resolved.map(|item| item.index),
                filesystem_uuid: resolved.and_then(|item| item.filesystem_uuid.clone()),
                gpt_partition_uuid: resolved.and_then(|item| item.gpt_partition_uuid.clone()),
            });
        }
    }
    Ok(results)
}

fn root_entries(text: &str) -> Vec<(u32, String, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 4 || decode_field(fields[1]).as_deref() != Some("/") {
                return None;
            }
            Some((
                index as u32 + 1,
                decode_field(fields[0])?,
                fields[2].to_string(),
            ))
        })
        .collect()
}

fn decode_field(field: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut index = 0;
    let input = field.as_bytes();
    while index < input.len() {
        if input[index] != b'\\' {
            bytes.push(input[index]);
            index += 1;
            continue;
        }
        let escaped = input.get(index + 1..index + 4)?;
        bytes.push(match escaped {
            b"040" => b' ',
            b"011" => b'\t',
            b"134" => b'\\',
            _ => return None,
        });
        index += 4;
    }
    String::from_utf8(bytes).ok()
}

fn matches_spec(
    spec: &str,
    partition: &DataSourcePartitionDto,
    record: Option<&persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord>,
) -> bool {
    let uuid = spec
        .strip_prefix("UUID=")
        .or_else(|| spec.strip_prefix("/dev/disk/by-uuid/"));
    if let Some(uuid) = uuid {
        return partition
            .filesystem_uuid
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(uuid));
    }
    let partuuid = spec
        .strip_prefix("PARTUUID=")
        .or_else(|| spec.strip_prefix("/dev/disk/by-partuuid/"));
    if let Some(uuid) = partuuid {
        return partition
            .gpt_partition_uuid
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(uuid));
    }
    let Some(record) = record else { return false };
    let (Some(vg), Some(lv)) = (&record.lvm_vg_name, &record.lvm_lv_name) else {
        return false;
    };
    spec == format!("/dev/{vg}/{lv}")
        || spec
            == format!(
                "/dev/mapper/{}-{}",
                vg.replace('-', "--"),
                lv.replace('-', "--")
            )
}

#[cfg(test)]
#[path = "../../tests/unit/file_service/linux_root_mounts.rs"]
mod tests;
