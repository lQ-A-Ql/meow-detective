use std::{collections::HashMap, path::Path};

use domain::{CaseId, DataSourceId, FileEntryId};
use evidence_core::{EvidenceReader, FileSystemReader};
use persistence_sqlite::repositories::{file_repo::FileRepo, partition_repo::PartitionRepo};
use rusqlite::Connection;
use transport::dto::BitLockerCatalogImportDto;
use volume_bitlocker::read_volume_identities;

use crate::{
    file_service::{self, EnumerationStats},
    import_pipeline::partition::{enumerate_partition_with_fs, PartitionEnumerationRequest},
};

use super::{
    audit::{self, BitLockerAudit},
    source::{
        open_partition_window, open_registered_plaintext, open_source_writable,
        probe_plaintext_filesystem, BitLockerSource,
    },
    status::{build_status, matching_identity},
    BitLockerRuntimeContext, BitLockerServiceError,
};

pub fn import_unlocked_bitlocker_catalog(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerCatalogImportDto, BitLockerServiceError> {
    let _read_lease = runtimes
        .preview_runtime
        .begin_session(case_id, data_source_id)?;
    let result = import_catalog_under_lease(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    );
    if let Ok(response) = &result {
        record_catalog_audit(
            case_conn,
            case_id,
            data_source_id,
            partition_index,
            &response.volume.metadata_fingerprint,
            if response.imported {
                "success"
            } else {
                "alreadyImported"
            },
        );
    }
    result
}

fn import_catalog_under_lease(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerCatalogImportDto, BitLockerServiceError> {
    let PreparedCatalog {
        source,
        plaintext,
        filesystem,
        mut status,
    } = prepare_catalog(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    )?;
    let root = find_partition_root(&source.source_conn, data_source_id, partition_index)?;
    if root.as_ref().is_some_and(|(_, placeholder)| !placeholder) {
        mark_catalog_ready(
            &source.source_conn,
            data_source_id,
            partition_index,
            &filesystem,
        )?;
        return Ok(catalog_response(status, false, None));
    }
    let placeholder = resolve_placeholder(&source, data_source_id, partition_index, root)?;
    let stats = enumerate_catalog(
        &source,
        data_source_id,
        partition_index,
        placeholder,
        plaintext,
        &filesystem,
    )?;
    mark_catalog_ready(
        &source.source_conn,
        data_source_id,
        partition_index,
        &filesystem,
    )?;
    status.plaintext_filesystem = Some(filesystem);
    Ok(catalog_response(status, true, Some(stats)))
}

struct PreparedCatalog {
    source: BitLockerSource,
    plaintext: Box<dyn EvidenceReader>,
    filesystem: String,
    status: transport::dto::BitLockerVolumeStatusDto,
}

fn prepare_catalog(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<PreparedCatalog, BitLockerServiceError> {
    let source = open_source_writable(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
    )?;
    let mut identity_window = open_partition_window(&source)?;
    let identities = read_volume_identities(&mut identity_window)?;
    let registered = runtimes.bitlocker_runtime.resolve_for_identities(
        &case_id.0,
        &data_source_id.0,
        partition_index as usize,
        &identities,
    )?;
    let identity = matching_identity(&identities, registered.scope().metadata_fingerprint());
    let stored_key_available = runtimes
        .key_store
        .contains(registered.scope().metadata_fingerprint())?;
    let mut plaintext = open_registered_plaintext(&source, case_id, runtimes.bitlocker_runtime)?;
    let filesystem = probe_plaintext_filesystem(plaintext.as_mut())?
        .ok_or_else(|| BitLockerServiceError::UnsupportedFilesystem("unknown".to_string()))?;
    let status = build_status(
        &data_source_id.0,
        partition_index,
        identity,
        identities.len(),
        true,
        stored_key_available,
        Some(filesystem.clone()),
    );
    Ok(PreparedCatalog {
        source,
        plaintext,
        filesystem,
        status,
    })
}

fn resolve_placeholder(
    source: &BitLockerSource,
    data_source_id: &DataSourceId,
    partition_index: u32,
    root: Option<(FileEntryId, bool)>,
) -> Result<FileEntryId, BitLockerServiceError> {
    let placeholder = match root {
        Some((id, true)) => id,
        Some((_, false)) => {
            return Err(BitLockerServiceError::CatalogState(
                "a real partition root reached placeholder replacement".to_string(),
            ));
        }
        None => file_service::insert_partition_placeholder_root(
            &source.source_conn,
            data_source_id,
            partition_index as usize,
            &source.partition.name,
            "unlocked",
        )?,
    };
    Ok(placeholder)
}

fn enumerate_catalog(
    source: &BitLockerSource,
    data_source_id: &DataSourceId,
    partition_index: u32,
    placeholder: FileEntryId,
    plaintext: Box<dyn EvidenceReader>,
    filesystem: &str,
) -> Result<EnumerationStats, BitLockerServiceError> {
    let fs = open_plaintext_filesystem(plaintext, filesystem)?;
    let placeholders = HashMap::from([(partition_index as usize, placeholder)]);
    let candidate = crate::datasource_service::ImageFilesystemCandidate {
        partition_index: Some(partition_index as usize),
        partition_name: Some(source.partition.name.clone()),
        kind: filesystem_kind(filesystem)?,
        offset: 0,
        length: (source.partition.length > 0).then_some(source.partition.length),
        source: crate::datasource_service::ImageFilesystemSource::DirectVolume,
        lvm_identity: None,
    };
    enumerate_partition_with_fs(PartitionEnumerationRequest {
        conn: &source.source_conn,
        data_source_id,
        fs: fs.as_ref(),
        root_name: &source.partition.name,
        placeholder_roots: &placeholders,
        candidate: &candidate,
        progress_cb: None,
        cancel_token: None,
    })
    .map_err(Into::into)
}

fn record_catalog_audit(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    metadata_fingerprint: &str,
    outcome: &str,
) {
    audit::record(
        case_conn,
        BitLockerAudit {
            case_id: &case_id.0,
            data_source_id: &data_source_id.0,
            partition_index,
            metadata_fingerprint: Some(metadata_fingerprint),
            operation: "catalogImport",
            outcome,
            error_code: None,
        },
    );
}

fn catalog_response(
    status: transport::dto::BitLockerVolumeStatusDto,
    imported: bool,
    stats: Option<EnumerationStats>,
) -> BitLockerCatalogImportDto {
    BitLockerCatalogImportDto {
        volume: status,
        imported,
        file_count: stats.as_ref().map(|value| value.file_count),
        directory_count: stats.as_ref().map(|value| value.dir_count),
        total_size: stats.as_ref().map(|value| value.total_size),
        warnings: stats.map_or_else(Vec::new, |value| value.warnings),
    }
}

fn find_partition_root(
    source_conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Result<Option<(FileEntryId, bool)>, BitLockerServiceError> {
    let repo = FileRepo::new(source_conn);
    let mut matches = Vec::new();
    for root in repo.find_roots(data_source_id)? {
        if repo.find_partition_index_by_id(&root.id)? == Some(partition_index as usize) {
            let placeholder = file_service::partition_placeholder_status(&root).is_some();
            matches.push((root.id, placeholder));
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        count => Err(BitLockerServiceError::CatalogState(format!(
            "partition {partition_index} has {count} root entries"
        ))),
    }
}

fn open_plaintext_filesystem(
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem: &str,
) -> Result<Box<dyn FileSystemReader + Send>, BitLockerServiceError> {
    match filesystem {
        "NTFS" => fs_ntfs::NtfsReader::open(reader, 0)
            .map(|value| Box::new(value) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| BitLockerServiceError::CatalogState(error.to_string())),
        "FAT" => fs_fat::FatReader::open(reader, 0)
            .map(|value| Box::new(value) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| BitLockerServiceError::CatalogState(error.to_string())),
        "EXFAT" => fs_exfat::ExfatReader::open(reader, 0)
            .map(|value| Box::new(value) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| BitLockerServiceError::CatalogState(error.to_string())),
        other => Err(BitLockerServiceError::UnsupportedFilesystem(
            other.to_string(),
        )),
    }
}

fn filesystem_kind(
    filesystem: &str,
) -> Result<crate::datasource_service::ImageFilesystemKind, BitLockerServiceError> {
    match filesystem {
        "NTFS" => Ok(crate::datasource_service::ImageFilesystemKind::Ntfs),
        "FAT" | "EXFAT" => Ok(crate::datasource_service::ImageFilesystemKind::Fat),
        other => Err(BitLockerServiceError::UnsupportedFilesystem(
            other.to_string(),
        )),
    }
}

fn mark_catalog_ready(
    source_conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: u32,
    filesystem: &str,
) -> Result<(), BitLockerServiceError> {
    let updated = PartitionRepo::new(source_conn).mark_bitlocker_catalog_ready(
        &data_source_id.0,
        partition_index,
        filesystem,
    )?;
    if updated == 1 {
        Ok(())
    } else {
        Err(BitLockerServiceError::CatalogState(format!(
            "partition {partition_index} metadata disappeared"
        )))
    }
}
