use std::path::Path;
use std::sync::Arc;

use domain::{CaseId, DataSourceId};
use evidence_mount::{MountPlan, MountReadPolicy, MountSession};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;

use crate::source_db::{open_ready_source_read_only_by_id, ReadySourceError};

use super::catalog::CatalogMountFileSystem;
use super::error::MountServiceError;
use super::filesystem_factory::open_partition_filesystem;
use super::source_validation::{
    mount_source_binding, validate_source_identity, validate_source_kind,
};

pub fn prepare_mount_session(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: usize,
    policy: MountReadPolicy,
) -> Result<MountSession, MountServiceError> {
    let fingerprint = mount_source_binding(
        data_source_id,
        DataSourceRepo::new(case_conn).source_fingerprint(data_source_id)?,
    );
    let source_path = DataSourceRepo::new(case_conn).source_path(data_source_id)?;
    let source_kind = DataSourceRepo::new(case_conn).source_kind(data_source_id)?;
    validate_source_kind(&source_kind)?;
    validate_source_identity(
        Path::new(&source_path),
        DataSourceRepo::new(case_conn).source_evidence_size(data_source_id)?,
    )?;
    let ready = open_ready_source_read_only_by_id(case_conn, case_root, case_id, data_source_id)
        .map_err(map_ready_error)?;
    let partition = PartitionRepo::new(&ready.connection)
        .find_by_data_source_and_index(&data_source_id.0, partition_index)?
        .ok_or_else(|| MountServiceError::NotFound(format!("partition {partition_index}")))?;
    validate_partition(&partition.status, partition.filesystem.as_deref())?;
    let filesystem = open_partition_filesystem(
        Path::new(&source_path),
        &source_kind,
        &case_id.0,
        &partition,
    )?;
    let plan = MountPlan::new(
        data_source_id.clone(),
        partition_index,
        partition
            .filesystem
            .as_deref()
            .unwrap_or(&partition.kind_label),
        fingerprint,
    )
    .map_err(|error| MountServiceError::Catalog(error.to_string()))?
    .with_volume_size(partition.length);
    let catalog = CatalogMountFileSystem::new(
        ready.connection,
        filesystem,
        data_source_id.clone(),
        partition_index,
    );
    let session = MountSession::new(plan, Arc::new(catalog), policy);
    session
        .read_directory(&evidence_mount::MountPath::root(), None, 1)
        .map_err(|error| MountServiceError::Catalog(error.to_string()))?;
    Ok(session)
}

fn validate_partition(status: &str, filesystem: Option<&str>) -> Result<(), MountServiceError> {
    if !matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "supported" | "queued" | "done" | "ready"
    ) {
        return Err(MountServiceError::SourceNotReady(format!(
            "partition status is '{status}'"
        )));
    }
    if filesystem.is_none_or(|kind| kind.trim().is_empty()) {
        return Err(MountServiceError::Unsupported(
            "partition has no filesystem kind".to_string(),
        ));
    }
    Ok(())
}

fn map_ready_error(error: ReadySourceError) -> MountServiceError {
    match error {
        ReadySourceError::Db(error) => MountServiceError::Database(error),
        ReadySourceError::NotFound { .. } => MountServiceError::NotFound("data source".to_string()),
        ReadySourceError::NotReady { state, .. } => MountServiceError::SourceNotReady(state),
        ReadySourceError::UnsupportedPlatform { reason, .. } => {
            MountServiceError::Unsupported(reason)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/open.rs"]
mod tests;
