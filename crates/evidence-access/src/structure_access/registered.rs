use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::{structure::ReadDemand, CaseId, DataSourceId, DataSourceKind, FileEntryId};
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo,
};
use source_kernel::source_runtime::{
    read_scope::SourceReadScope,
    verification::{ValidatedReceipt, VerificationBatch, VerificationRequest},
};

use super::{TargetAccessError, TargetAccessPlan};

pub const CEPHFS_FILE_TARGET_PREFIX: &str = "cephfs-file-v1:";
pub const REGISTERED_READ_POLICY: &str = "registered-range-v1";

pub struct RegisteredAccessRequest {
    pub demand: ReadDemand,
    pub file_id: Option<FileEntryId>,
}

pub fn plan_registered_access(
    conn: &rusqlite::Connection,
    root: &Path,
    request: RegisteredAccessRequest,
    cancel: &AtomicBool,
) -> Result<TargetAccessPlan, TargetAccessError> {
    super::plan::check_cancel(cancel)?;
    source_kernel::source_runtime::range_view::check_active(
        cancel,
        request.demand.deadline_unix_ms,
    )?;
    let scope = SourceReadScope::acquire(root, &request.demand.source_revision)?;
    validate_target(conn, &request)?;
    let reader = scope.wrap(open_reader(conn, root, &request, cancel)?)?;
    Ok(TargetAccessPlan {
        demand: request.demand,
        reader,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn verify_registered_access(
    conn: &rusqlite::Connection,
    root: &Path,
    request: RegisteredAccessRequest,
    batch: &VerificationBatch,
    adapter_version: &str,
    expected_sha256: Option<&str>,
    fresh: bool,
    cancel: &AtomicBool,
) -> Result<Arc<ValidatedReceipt>, TargetAccessError> {
    super::plan::check_cancel(cancel)?;
    source_kernel::source_runtime::range_view::check_active(
        cancel,
        request.demand.deadline_unix_ms,
    )?;
    validate_target(conn, &request)?;
    let scope = SourceReadScope::acquire(root, &request.demand.source_revision)?;
    if fresh || batch.requires_input_verification() {
        super::deep::verify_inputs(conn, root, &request.demand, batch, cancel)?;
    }
    Ok(batch.verify(
        scope,
        VerificationRequest {
            demand: &request.demand,
            adapter_version,
            policy_version: REGISTERED_READ_POLICY,
            expected_sha256,
            fresh,
        },
        cancel,
        || open_reader(conn, root, &request, cancel).map_err(std::io::Error::other),
    )?)
}

fn validate_target(
    conn: &rusqlite::Connection,
    request: &RegisteredAccessRequest,
) -> Result<(), TargetAccessError> {
    let source = &request.demand.source_revision;
    let revisions = SourceRevisionRepo::new(conn);
    revisions.require_current(source)?;
    request.demand.ranges.validate()?;
    if let Some(file) = &request.file_id {
        if request.demand.target_fingerprint != format!("{CEPHFS_FILE_TARGET_PREFIX}{}", file.0) {
            return Err(TargetAccessError::UnsupportedTarget);
        }
        return Ok(());
    }
    let version =
        StorageObjectRepo::new(conn).version_for_source(&source.case_id, &source.source_id)?;
    if version
        .as_ref()
        .is_some_and(|version| version.mapping_sha256 == request.demand.target_fingerprint)
        || request.demand.target_fingerprint
            == format!("source-revision:{}:{}", source.source_id, source.revision)
    {
        return Ok(());
    }
    for partition in crate::partition_query_service::PartitionQueryRepo::new(conn)
        .find_by_data_source(&source.source_id)?
    {
        let fingerprint = format!(
            "filesystem-window-v1:{}:{}:{}",
            source.source_id,
            partition.partition_index,
            partition.filesystem.as_deref().unwrap_or_default()
        );
        if fingerprint != request.demand.target_fingerprint {
            continue;
        }
        let end = partition
            .offset
            .checked_add(partition.length)
            .ok_or(TargetAccessError::UnsupportedTarget)?;
        if request
            .demand
            .ranges
            .ranges()
            .iter()
            .all(|range| range.start() >= partition.offset && range.end() <= end)
        {
            return Ok(());
        }
    }
    Err(TargetAccessError::UnsupportedTarget)
}

fn open_reader(
    conn: &rusqlite::Connection,
    root: &Path,
    request: &RegisteredAccessRequest,
    cancel: &AtomicBool,
) -> Result<Box<dyn EvidenceReader>, TargetAccessError> {
    let reference = &request.demand.source_revision;
    let source_id = DataSourceId(reference.source_id.clone());
    let case_id = CaseId(reference.case_id.clone());
    let repo = DataSourceRepo::new(conn);
    if repo.case_id(&source_id)? != case_id {
        return Err(TargetAccessError::UnsupportedTarget);
    }
    let kind = repo.source_kind(&source_id)?;
    if matches!(kind, DataSourceKind::Raw | DataSourceKind::E01) {
        SourceRevisionRepo::new(conn).require_current(reference)?;
    } else if repo
        .find_storage(&source_id)?
        .is_some_and(|storage| storage.import_state == "ready_metadata")
    {
        drop(
            source_kernel::source_db::open_reconstruction_source_by_id(
                conn, root, &case_id, &source_id,
            )
            .map_err(|error| error.into_db_error())?,
        );
    } else {
        drop(
            source_kernel::source_db::open_ready_source_read_only_by_id(
                conn, root, &case_id, &source_id,
            )
            .map_err(|error| error.into_db_error())?,
        );
    }
    if kind == DataSourceKind::CephFs {
        let file_id = request
            .file_id
            .as_ref()
            .ok_or(TargetAccessError::UnsupportedTarget)?;
        return crate::file_service::cephfs_adapter::open_evidence_file(
            conn, root, reference, file_id,
        )
        .map_err(|error| TargetAccessError::Io(std::io::Error::other(error)));
    }
    if request.file_id.is_some() {
        return Err(TargetAccessError::UnsupportedTarget);
    }
    if kind == DataSourceKind::CephRbd {
        return ceph_reconstruction::ceph_reconstruction::open_derived_rbd_reader(
            conn, root, &case_id, &source_id,
        )
        .map_err(|error| TargetAccessError::Io(std::io::Error::other(error)));
    }
    let path = std::path::PathBuf::from(repo.source_path(&source_id)?);
    match kind {
        DataSourceKind::MdRaid => crate::mdraid::persisted_inputs::validate(&source_id, &path)?,
        DataSourceKind::LvmLogicalVolume => {
            crate::lvm::persisted_inputs::validate(&source_id, &path)
                .map_err(|error| TargetAccessError::Io(std::io::Error::other(error)))?
        }
        DataSourceKind::Raw | DataSourceKind::E01 | DataSourceKind::LocalDisk => {}
        _ => return Err(TargetAccessError::UnsupportedTarget),
    }
    let factory =
        crate::source_runtime::factory::open_factory_for_case(&path, &kind, cancel, &case_id.0)?;
    if matches!(
        kind,
        DataSourceKind::MdRaid | DataSourceKind::LvmLogicalVolume
    ) {
        let version = StorageObjectRepo::new(conn)
            .version_for_source(&case_id.0, &source_id.0)?
            .ok_or(TargetAccessError::UnsupportedTarget)?;
        if factory.metadata_identity() != Some(version.mapping_sha256.as_str()) {
            return Err(TargetAccessError::UnsupportedTarget);
        }
    }
    Ok(factory.open_reader_with_cancel(cancel)?)
}
