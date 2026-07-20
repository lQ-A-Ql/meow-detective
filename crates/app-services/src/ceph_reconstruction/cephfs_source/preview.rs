use std::path::Path;

use ceph_wire::CephFsFileLayout;
use domain::{CaseId, DataSourceId};

use super::{
    lineage::load_runtime,
    materialization::{read_capability_record, validate_assembly_record},
    CephFsSourceCapability, CephFsSourceError, CephFsSourceResult,
};
use crate::{
    ceph_reconstruction::{
        CephFsDataRangeReader, CephFsFileDataContent, CephFsFileDataDescriptor,
        SourceDbCephFsObjectReader,
    },
    file_service::PreviewDescriptor,
};

pub(crate) struct PreparedCephFsFileReader {
    reader: CephFsDataRangeReader<SourceDbCephFsObjectReader>,
    size: u64,
    projection_sha256: String,
    lineage_fingerprint: String,
}

impl PreparedCephFsFileReader {
    pub(crate) fn read_range(&mut self, offset: u64, length: usize) -> CephFsSourceResult<Vec<u8>> {
        if offset > self.size {
            return Err(CephFsSourceError::InvalidInput(
                "preview offset exceeds file size",
            ));
        }
        let length =
            length.min(usize::try_from(self.size.saturating_sub(offset)).unwrap_or(usize::MAX));
        self.reader
            .read_range(offset, length)
            .map(|range| range.bytes)
            .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))
    }

    pub(crate) fn projection_sha256(&self) -> &str {
        &self.projection_sha256
    }

    pub(crate) fn lineage_fingerprint(&self) -> &str {
        &self.lineage_fingerprint
    }
}

pub(crate) fn open_cephfs_file_reader(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    descriptor: &PreviewDescriptor,
) -> CephFsSourceResult<PreparedCephFsFileReader> {
    if descriptor.source_kind != "ceph_fs" || descriptor.data_source_id != data_source_id.0 {
        return Err(CephFsSourceError::InvalidInput(
            "preview descriptor is not bound to this CephFS source",
        ));
    }
    let file = descriptor
        .ceph_fs
        .as_ref()
        .ok_or(CephFsSourceError::InvalidInput(
            "CephFS preview descriptor is missing",
        ))?;
    let runtime = load_runtime(case_conn, case_root, case_id, data_source_id, file.pool_id)?;
    if runtime.descriptor.identity != file.filesystem_identity
        || runtime.descriptor.filesystem_id != file.filesystem_id
        || runtime.descriptor.fsmap_epoch != file.fsmap_epoch
    {
        return Err(CephFsSourceError::InconsistentState(
            "CephFS preview descriptor does not match lineage".to_string(),
        ));
    }
    let aggregate =
        persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRepo::new(
            case_conn,
        )
        .find_by_data_source(&data_source_id.0)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState("CephFS lineage is missing".to_string())
        })?;
    if aggregate.lineage.namespace_projection_sha256 != file.projection_sha256 {
        return Err(CephFsSourceError::InconsistentState(
            "CephFS namespace projection is stale".to_string(),
        ));
    }
    let validation_connection = crate::source_db::open_registered_source_db_read_only(
        case_conn,
        case_root,
        data_source_id,
    )?;
    validate_assembly_record(
        &validation_connection,
        &aggregate.lineage,
        &data_source_id.0,
        true,
    )?;
    let capability = read_capability_record(
        &validation_connection,
        &aggregate.lineage,
        &data_source_id.0,
    )?;
    if capability != CephFsSourceCapability::BoundedPreview {
        return Err(CephFsSourceError::CapabilityInsufficient {
            required: "bounded-preview",
            actual: capability.as_str().to_string(),
        });
    }
    let layout = CephFsFileLayout::new(
        file.stripe_unit,
        file.stripe_count,
        file.object_size,
        runtime.resolved_pool_id,
        file.pool_namespace.clone(),
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let data_descriptor = CephFsFileDataDescriptor::with_content(
        file.filesystem_identity.clone(),
        file.filesystem_id,
        file.fsmap_epoch,
        file.inode,
        descriptor.size,
        layout,
        CephFsFileDataContent {
            inline_data: file.inline_data.clone(),
            sparse_extents: file.sparse_extents.clone(),
        },
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let object_reader = SourceDbCephFsObjectReader::for_data_pool(
        runtime.descriptor,
        runtime.sources,
        runtime.expected_replica_count,
        runtime.resolved_pool_id,
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let reader = CephFsDataRangeReader::new(data_descriptor, object_reader)
        .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    Ok(PreparedCephFsFileReader {
        reader,
        size: descriptor.size,
        projection_sha256: file.projection_sha256.clone(),
        lineage_fingerprint: runtime.lineage_fingerprint,
    })
}
