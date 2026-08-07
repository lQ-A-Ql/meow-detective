//! Host-side SAM bypass for emulation sessions.
//!
//! The SAM hive is read from the evidence image (read-only), edited in
//! memory with same-size cell writes, and written back through the session's
//! copy-on-write overlay disk. The evidence image is never written: the only
//! writable parameter in this module is the session `CowDisk`.

use domain::{CaseId, DataSourceId};
use evidence_emulation::CowDisk;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use transport::dto::{
    EmulationBypassAccountDto, EmulationBypassActionDto, EmulationBypassResultDto,
};

const SYSTEM_HIVE_PATH: &str = "Windows/System32/config/SYSTEM";
const SAM_HIVE_PATH: &str = "Windows/System32/config/SAM";

#[derive(Debug, thiserror::Error)]
pub enum EmulationBypassError {
    #[error("database error: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("evidence read failed: {0}")]
    EvidenceRead(String),
    #[error("emulation overlay write failed: {0}")]
    OverlayWrite(String),
    #[error("partition {partition_index} was not found on the data source")]
    PartitionNotFound { partition_index: u32 },
    #[error("the target filesystem cannot be edited: {0}")]
    Unsupported(String),
    #[error("SAM edit failed: {0}")]
    Edit(String),
}

impl transport::ServiceErrorCategory for EmulationBypassError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Database(_) | Self::EvidenceRead(_) | Self::OverlayWrite(_) => {
                transport::ErrorCategory::Io
            }
            Self::PartitionNotFound { .. } => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Edit(_) => transport::ErrorCategory::Parser,
        }
    }
}

pub(crate) struct PartitionFilesystem {
    pub(crate) fs: fs_ntfs::NtfsReader,
    pub(crate) partition_offset: u64,
}

/// Everything the bypass flow needs to reach the case and the source catalog.
pub struct BypassCaseContext<'a> {
    pub case_conn: &'a Connection,
    pub case_root: &'a std::path::Path,
    pub case_id: &'a CaseId,
    pub data_source_id: &'a DataSourceId,
}

pub fn list_bypass_accounts(
    context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<Vec<EmulationBypassAccountDto>, EmulationBypassError> {
    let context = open_partition_filesystem(context, partition_index)?;
    let sam = read_whole_file(&context.fs, SAM_HIVE_PATH)?;
    let accounts = artifacts_windows::registry::sam_edit::list_accounts(&sam)
        .map_err(EmulationBypassError::Edit)?;
    Ok(accounts
        .into_iter()
        .map(|account| EmulationBypassAccountDto {
            rid: account.rid,
            username: account.username,
            disabled: account.disabled,
            locked_out: account.locked_out,
            has_password: account.has_password,
        })
        .collect())
}

pub fn apply_bypass(
    disk: &CowDisk,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    rid: u32,
    action: EmulationBypassActionDto,
) -> Result<EmulationBypassResultDto, EmulationBypassError> {
    let context = open_partition_filesystem(case_context, partition_index)?;
    let system = read_whole_file(&context.fs, SYSTEM_HIVE_PATH)?;
    let mut sam = read_whole_file(&context.fs, SAM_HIVE_PATH)?;
    let hbootkey = artifacts_windows::registry::sam_edit::derive_hbootkey_from_hives(&system, &sam)
        .ok_or_else(|| {
            EmulationBypassError::Edit(
                "could not derive the SAM hashed boot key from the hives".to_string(),
            )
        })?;
    let username = artifacts_windows::registry::sam_edit::list_accounts(&sam)
        .map_err(EmulationBypassError::Edit)?
        .into_iter()
        .find(|account| account.rid == rid)
        .map(|account| account.username)
        .ok_or_else(|| {
            EmulationBypassError::Edit(format!("account RID {rid} was not found in the SAM hive"))
        })?;
    let edit_action = match action {
        EmulationBypassActionDto::ClearPassword => {
            artifacts_windows::registry::sam_edit::SamBypassAction::ClearPassword
        }
        EmulationBypassActionDto::EnableAndClearPassword => {
            artifacts_windows::registry::sam_edit::SamBypassAction::EnableAndClearPassword
        }
    };
    let outcome =
        artifacts_windows::registry::sam_edit::apply_bypass(&mut sam, rid, edit_action, &hbootkey)
            .map_err(EmulationBypassError::Edit)?;
    write_hive_through_overlay(disk, &context, &sam)?;
    verify_overlay_write(disk, &context, &sam)?;
    Ok(EmulationBypassResultDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        rid,
        username,
        password_cleared: outcome.password_cleared,
        account_enabled: outcome.account_enabled,
        already_passwordless: outcome.already_passwordless,
    })
}

pub(crate) fn open_partition_filesystem(
    context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<PartitionFilesystem, EmulationBypassError> {
    let source = DataSourceRepo::new(context.case_conn)
        .find_by_case(context.case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *context.data_source_id)
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    let source_conn = crate::source_db::open_ready_source_read_only_by_id(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let partition = PartitionRepo::new(&source_conn.connection)
        .find_by_data_source_and_index(&context.data_source_id.0, partition_index as usize)?
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    let reader = crate::datasource_service::open_evidence_reader(&source.source_path, &source.kind)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let length = (partition.length > 0).then_some(partition.length);
    let window = evidence_core::PartitionWindowReader::new(reader, partition.offset, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let fs = fs_ntfs::NtfsReader::open(Box::new(window), 0)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    Ok(PartitionFilesystem {
        fs,
        partition_offset: partition.offset,
    })
}

fn read_whole_file(fs: &fs_ntfs::NtfsReader, path: &str) -> Result<Vec<u8>, EmulationBypassError> {
    let inode = fs
        .preview_file(path)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?
        .inode();
    let size = fs
        .file_size_by_inode(inode)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?
        .ok_or_else(|| EmulationBypassError::Unsupported(format!("{path} has no data stream")))?;
    let length = usize::try_from(size)
        .map_err(|_| EmulationBypassError::Unsupported(format!("{path} is too large")))?;
    fs.read_file_range(path, 0, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))
}

fn write_hive_through_overlay(
    disk: &CowDisk,
    context: &PartitionFilesystem,
    hive: &[u8],
) -> Result<(), EmulationBypassError> {
    let extents = context
        .fs
        .file_extent_map(SAM_HIVE_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    for extent in extents {
        let start = usize::try_from(extent.logical_offset)
            .map_err(|_| EmulationBypassError::Unsupported("extent offset too large".into()))?;
        let end = start.saturating_add(extent.length as usize).min(hive.len());
        if start >= end {
            continue;
        }
        let absolute = context
            .partition_offset
            .checked_add(extent.volume_offset)
            .ok_or_else(|| EmulationBypassError::Unsupported("extent address overflows".into()))?;
        disk.write_all_at(absolute, &hive[start..end])
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
    }
    disk.flush()
        .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))
}

fn verify_overlay_write(
    disk: &CowDisk,
    context: &PartitionFilesystem,
    hive: &[u8],
) -> Result<(), EmulationBypassError> {
    let mut readback = vec![0u8; hive.len()];
    let extents = context
        .fs
        .file_extent_map(SAM_HIVE_PATH)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    for extent in extents {
        let start = usize::try_from(extent.logical_offset)
            .map_err(|_| EmulationBypassError::Unsupported("extent offset too large".into()))?;
        let end = start.saturating_add(extent.length as usize).min(hive.len());
        if start >= end {
            continue;
        }
        let absolute = context
            .partition_offset
            .checked_add(extent.volume_offset)
            .ok_or_else(|| EmulationBypassError::Unsupported("extent address overflows".into()))?;
        disk.read_exact_at(absolute, &mut readback[start..end])
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?;
    }
    if readback != hive {
        return Err(EmulationBypassError::OverlayWrite(
            "overlay read-back does not match the edited hive".to_string(),
        ));
    }
    Ok(())
}
