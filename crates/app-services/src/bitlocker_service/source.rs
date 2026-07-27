use std::io::SeekFrom;
use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use evidence_core::{EvidenceReader, PartitionWindowReader};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use rusqlite::Connection;

use super::BitLockerServiceError;

pub(crate) struct BitLockerSource {
    pub source: DataSource,
    pub partition: DataSourcePartitionRecord,
    pub source_conn: Connection,
}

pub(crate) fn open_source_read_only(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Result<BitLockerSource, BitLockerServiceError> {
    open_source(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        true,
    )
}

pub(crate) fn open_source_writable(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Result<BitLockerSource, BitLockerServiceError> {
    open_source(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        false,
    )
}

fn open_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    read_only: bool,
) -> Result<BitLockerSource, BitLockerServiceError> {
    let source = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *data_source_id)
        .ok_or_else(|| {
            BitLockerServiceError::Source(crate::source_db::ReadySourceError::NotFound {
                case_id: case_id.0.clone(),
                data_source_id: data_source_id.0.clone(),
            })
        })?;
    validate_source_kind(&source.kind)?;
    let manager = crate::source_db::SourceConnectionManager::new(case_root);
    let source_conn = if read_only {
        manager.open_ready_read_only(case_conn, case_id, data_source_id)?
    } else {
        manager.open_ready(case_conn, case_id, data_source_id)?
    };
    let partition = PartitionRepo::new(&source_conn)
        .find_by_data_source_and_index(&data_source_id.0, partition_index as usize)?
        .ok_or_else(|| BitLockerServiceError::PartitionNotFound {
            data_source_id: data_source_id.0.clone(),
            partition_index,
        })?;
    if !is_bitlocker_partition(&partition) {
        return Err(BitLockerServiceError::NotBitLocker { partition_index });
    }
    Ok(BitLockerSource {
        source,
        partition,
        source_conn,
    })
}

pub(crate) fn open_partition_window(
    source: &BitLockerSource,
) -> Result<PartitionWindowReader, BitLockerServiceError> {
    let reader = crate::datasource_service::open_evidence_reader(
        &source.source.source_path,
        &source.source.kind,
    )
    .map_err(BitLockerServiceError::EvidenceOpen)?;
    let length = (source.partition.length > 0).then_some(source.partition.length);
    PartitionWindowReader::new(reader, source.partition.offset, length)
        .map_err(BitLockerServiceError::InvalidWindow)
}

pub(crate) fn open_registered_plaintext(
    source: &BitLockerSource,
    case_id: &CaseId,
    registry: &std::sync::Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
) -> Result<Box<dyn EvidenceReader>, BitLockerServiceError> {
    let reader = crate::datasource_service::open_evidence_reader(
        &source.source.source_path,
        &source.source.kind,
    )
    .map_err(BitLockerServiceError::EvidenceOpen)?;
    let length = (source.partition.length > 0).then_some(source.partition.length);
    crate::bitlocker_runtime::open_registered_bitlocker_volume(
        reader,
        source.partition.offset,
        length,
        &case_id.0,
        &source.source.id.0,
        source.partition.partition_index as usize,
        registry,
    )
    .map_err(Into::into)
}

pub(crate) fn probe_plaintext_filesystem(
    reader: &mut dyn EvidenceReader,
) -> Result<Option<String>, BitLockerServiceError> {
    reader
        .seek(SeekFrom::Start(0))
        .map_err(BitLockerServiceError::InvalidWindow)?;
    let mut sector = [0u8; 512];
    reader
        .read_exact(&mut sector)
        .map_err(BitLockerServiceError::InvalidWindow)?;
    if &sector[3..11] == b"EXFAT   " && sector[510..512] == [0x55, 0xAA] {
        return Ok(Some("EXFAT".to_string()));
    }
    reader
        .seek(SeekFrom::Start(0))
        .map_err(BitLockerServiceError::InvalidWindow)?;
    let kind = crate::datasource_service::read_boot_filesystem(reader, 0)
        .map_err(|error| BitLockerServiceError::CatalogState(error.to_string()))?;
    Ok(kind.map(|value| match value {
        crate::datasource_service::ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        crate::datasource_service::ImageFilesystemKind::Fat => "FAT".to_string(),
        crate::datasource_service::ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
        crate::datasource_service::ImageFilesystemKind::Ext4 => "EXT4".to_string(),
        crate::datasource_service::ImageFilesystemKind::Xfs => "XFS".to_string(),
        crate::datasource_service::ImageFilesystemKind::Btrfs => "BTRFS".to_string(),
        crate::datasource_service::ImageFilesystemKind::LvmPool => "LVM".to_string(),
    }))
}

fn validate_source_kind(kind: &DataSourceKind) -> Result<(), BitLockerServiceError> {
    if matches!(kind, DataSourceKind::E01 | DataSourceKind::Raw) {
        return Ok(());
    }
    Err(BitLockerServiceError::UnsupportedSourceKind {
        kind: kind.to_string(),
    })
}

pub(crate) fn is_bitlocker_partition(partition: &DataSourcePartitionRecord) -> bool {
    partition.kind_label.eq_ignore_ascii_case("bitlocker")
        || partition
            .filesystem
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("bitlocker"))
        || matches!(
            partition.status.to_ascii_lowercase().as_str(),
            "locked" | "encrypted_bitlocker"
        )
}
