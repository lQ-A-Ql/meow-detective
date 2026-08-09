//! Host-side Linux logon bypass for emulation sessions.
//!
//! Clearing a Linux account's password empties the hash field of its
//! `/etc/shadow` line, which shortens the file. The service rewrites the
//! shadow content over its existing extents through the session's
//! copy-on-write overlay and truncates the file by shrinking the inode's
//! `i_size` in place. The evidence image is never written: the only
//! writable parameter in this module is the session `CowDisk`. Only ext4
//! roots (direct partitions or LVM logical volumes) are editable; XFS and
//! btrfs are typed `Unsupported` for now.

use std::sync::Arc;

use evidence_core::FileSystemReader;
use evidence_emulation::CowDisk;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};
use transport::dto::{EmulationLinuxAccountDto, EmulationLinuxBypassResultDto};

use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};

const SHADOW_PATH: &str = "etc/shadow";
const I_SIZE_LO_OFFSET: u64 = 0x04;
const I_SIZE_HI_OFFSET: u64 = 0x6C;

/// How a volume-relative byte offset translates to an absolute disk offset.
enum WriteMapping {
    Direct {
        partition_offset: u64,
    },
    /// LVM extent physical offsets are already disk-absolute in the
    /// coordinate space of the readers the pool was discovered from.
    Lvm {
        extents: Vec<fs_lvm::LvExtent>,
    },
}

struct LinuxExt4Partition {
    fs: fs_ext4::Ext4Reader,
    mapping: WriteMapping,
}

pub fn list_linux_accounts(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<Vec<EmulationLinuxAccountDto>, EmulationBypassError> {
    let partition = open_linux_ext4(case_context, partition_index, None)?;
    let shadow = read_shadow(&partition.fs)?;
    let accounts = artifacts_linux::parse_shadow_accounts(&shadow)
        .into_iter()
        .map(|account| EmulationLinuxAccountDto {
            username: account.username,
            has_password: account.has_password,
            locked: account.locked,
        })
        .collect();
    Ok(accounts)
}

pub fn apply_linux_bypass(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
) -> Result<EmulationLinuxBypassResultDto, EmulationBypassError> {
    // Read through the overlay so bypasses of multiple accounts in one
    // session compose (same rule as the SAM bypass).
    let partition = open_linux_ext4(case_context, partition_index, Some(disk))?;
    let shadow = read_shadow(&partition.fs)?;
    let accounts = artifacts_linux::parse_shadow_accounts(&shadow);
    let account = accounts
        .iter()
        .find(|account| account.username == username)
        .ok_or_else(|| {
            EmulationBypassError::Edit(format!("account {username} was not found in /etc/shadow"))
        })?;
    let result = |password_cleared, already_passwordless| EmulationLinuxBypassResultDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        username: username.to_string(),
        password_cleared,
        already_passwordless,
    };
    if !account.has_password {
        return Ok(result(false, true));
    }
    let edited = artifacts_linux::clear_shadow_password(&shadow, username)
        .map_err(|error| EmulationBypassError::Edit(error.to_string()))?
        .ok_or_else(|| {
            EmulationBypassError::Edit(format!("account {username} is already passwordless"))
        })?;
    write_shadow_through_overlay(disk, &partition, edited.as_bytes())?;
    verify_shadow_write(disk, case_context, partition_index, &edited, username)?;
    Ok(result(true, false))
}

/// Rewrite the shadow file over its extents, zero the tail left by the
/// shorter content, then shrink `i_size` in the inode record.
fn write_shadow_through_overlay(
    disk: &Arc<CowDisk>,
    partition: &LinuxExt4Partition,
    content: &[u8],
) -> Result<(), EmulationBypassError> {
    let fs = &partition.fs;
    let old_len = fs
        .file_size_by_path(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if content.len() as u64 > old_len {
        return Err(EmulationBypassError::Unsupported(
            "the edited shadow file cannot grow in place".to_string(),
        ));
    }
    let extents = fs
        .file_extent_map(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    write_bytes_over_extents(disk, &partition.mapping, &extents, content)?;
    if (content.len() as u64) < old_len {
        write_tail_zeroes(
            disk,
            &partition.mapping,
            &extents,
            content.len() as u64,
            old_len,
        )?;
    }
    let new_len = content.len() as u32;
    let inode_offset = fs
        .inode_source_offset(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    write_absolute(
        disk,
        &partition.mapping,
        inode_offset + I_SIZE_LO_OFFSET,
        &new_len.to_le_bytes(),
    )?;
    write_absolute(
        disk,
        &partition.mapping,
        inode_offset + I_SIZE_HI_OFFSET,
        &0u32.to_le_bytes(),
    )?;
    disk.flush()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))
}

fn write_tail_zeroes(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    extents: &[fs_ext4::Ext4FileExtent],
    new_len: u64,
    old_len: u64,
) -> Result<(), EmulationBypassError> {
    for extent in extents {
        let overlap_start = extent.logical_offset.max(new_len);
        let overlap_end = (extent.logical_offset + extent.length).min(old_len);
        if overlap_start >= overlap_end {
            continue;
        }
        let physical = extent
            .volume_offset
            .checked_add(overlap_start - extent.logical_offset)
            .ok_or_else(|| EmulationBypassError::Edit("extent address overflows".to_string()))?;
        write_absolute(
            disk,
            mapping,
            physical,
            &vec![0u8; (overlap_end - overlap_start) as usize],
        )?;
    }
    Ok(())
}

fn write_bytes_over_extents(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    extents: &[fs_ext4::Ext4FileExtent],
    content: &[u8],
) -> Result<(), EmulationBypassError> {
    for extent in extents {
        let start = extent.logical_offset as usize;
        let end = start
            .saturating_add(extent.length as usize)
            .min(content.len());
        if start >= end {
            continue;
        }
        write_absolute(disk, mapping, extent.volume_offset, &content[start..end])?;
    }
    Ok(())
}

fn write_absolute(
    disk: &Arc<CowDisk>,
    mapping: &WriteMapping,
    volume_offset: u64,
    bytes: &[u8],
) -> Result<(), EmulationBypassError> {
    let absolute = translate(mapping, volume_offset)?;
    disk.write_all_at(absolute, bytes)
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))
}

fn translate(mapping: &WriteMapping, volume_offset: u64) -> Result<u64, EmulationBypassError> {
    match mapping {
        WriteMapping::Direct { partition_offset } => partition_offset
            .checked_add(volume_offset)
            .ok_or_else(|| EmulationBypassError::Edit("extent address overflows".to_string())),
        WriteMapping::Lvm { extents } => {
            let index = extents
                .partition_point(|extent| extent.logical_start <= volume_offset)
                .checked_sub(1)
                .ok_or_else(|| {
                    EmulationBypassError::Edit("offset below the LV extent map".into())
                })?;
            let extent = &extents[index];
            if volume_offset >= extent.logical_start + extent.length {
                return Err(EmulationBypassError::Edit(
                    "offset is not covered by the LV extent map".to_string(),
                ));
            }
            Ok(extent.physical_offset + (volume_offset - extent.logical_start))
        }
    }
}

/// Byte read-back plus a semantic re-parse through a fresh overlay view:
/// the target account's hash field must be empty and the file must read at
/// the truncated length.
fn verify_shadow_write(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    expected: &str,
    username: &str,
) -> Result<(), EmulationBypassError> {
    let partition = open_linux_ext4(case_context, partition_index, Some(disk))?;
    let reread = read_shadow(&partition.fs)?;
    if reread != expected {
        return Err(EmulationBypassError::OverlayWrite(
            "overlay shadow read-back does not match the edited content".to_string(),
        ));
    }
    let accounts = artifacts_linux::parse_shadow_accounts(&reread);
    match accounts.iter().find(|account| account.username == username) {
        Some(account) if !account.has_password => Ok(()),
        _ => Err(EmulationBypassError::OverlayWrite(
            "the edited shadow still lists a password hash for the account".to_string(),
        )),
    }
}

fn read_shadow(fs: &fs_ext4::Ext4Reader) -> Result<String, EmulationBypassError> {
    let size = fs
        .file_size_by_path(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let length = usize::try_from(size)
        .map_err(|_| EmulationBypassError::Unsupported("shadow file is too large".into()))?;
    let bytes = fs
        .read_file_range(SHADOW_PATH, 0, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|error| EmulationBypassError::Edit(format!("shadow is not UTF-8: {error}")))
}

fn open_linux_ext4(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    overlay: Option<&Arc<CowDisk>>,
) -> Result<LinuxExt4Partition, EmulationBypassError> {
    let source = DataSourceRepo::new(case_context.case_conn)
        .find_by_case(case_context.case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *case_context.data_source_id)
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    let source_conn = crate::source_db::open_ready_source_read_only_by_id(
        case_context.case_conn,
        case_context.case_root,
        case_context.case_id,
        case_context.data_source_id,
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let record = PartitionRepo::new(&source_conn.connection)
        .find_by_data_source_and_index(&case_context.data_source_id.0, partition_index as usize)?
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    if record.filesystem.as_deref() != Some("Ext4") {
        return Err(EmulationBypassError::Unsupported(format!(
            "partition filesystem {:?} is not ext4; Linux bypass is ext4-only for now",
            record.filesystem
        )));
    }
    let reader: Box<dyn evidence_core::EvidenceReader> = match overlay {
        Some(disk) => Box::new(crate::emulation_cow_reader::CowDiskReader::new(Arc::clone(
            disk,
        ))),
        None => crate::datasource_service::open_evidence_reader(&source.source_path, &source.kind)
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?,
    };
    if record.lvm_lv_name.is_some() {
        open_lvm_ext4(reader, &record)
    } else {
        let length = (record.length > 0).then_some(record.length);
        let window = evidence_core::PartitionWindowReader::new(reader, record.offset, length)
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
        let fs = fs_ext4::Ext4Reader::open(Box::new(window), 0)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
        Ok(LinuxExt4Partition {
            fs,
            mapping: WriteMapping::Direct {
                partition_offset: record.offset,
            },
        })
    }
}

fn open_lvm_ext4(
    reader: Box<dyn evidence_core::EvidenceReader>,
    record: &persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord,
) -> Result<LinuxExt4Partition, EmulationBypassError> {
    let pv_offsets: Vec<u64> = record
        .lvm_pv_offsets_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .filter(|offsets: &Vec<u64>| !offsets.is_empty())
        .ok_or_else(|| {
            EmulationBypassError::Unsupported("LV has no persisted PV offsets".to_string())
        })?;
    let pool = fs_lvm::LvmPool::discover(vec![reader], pv_offsets)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let volumes = pool.list_volumes();
    let lv_index = volumes
        .iter()
        .position(|volume| {
            Some(volume.name.as_str()) == record.lvm_lv_name.as_deref()
                && Some(volume.uuid.as_str()) == record.lvm_lv_uuid.as_deref()
        })
        .ok_or_else(|| EmulationBypassError::PartitionNotFound {
            partition_index: record.partition_index,
        })?;
    let lv = pool
        .open_volume(lv_index)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let extents = lv.extent_map().to_vec();
    let fs = fs_ext4::Ext4Reader::open(Box::new(lv), 0)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    Ok(LinuxExt4Partition {
        fs,
        mapping: WriteMapping::Lvm { extents },
    })
}

#[cfg(test)]
#[path = "../tests/unit/emulation_linux_bypass.rs"]
mod tests;
