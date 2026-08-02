use std::path::Path;

use ceph_wire::CephFsFileLayout;
use domain::{CaseId, DataSourceId};

use super::{
    lineage::load_runtime,
    materialization::{read_capability_record, validate_assembly_record},
    CephFsSourceCapability, CephFsSourceError, CephFsSourceResult,
};
use crate::ceph_reconstruction::{
    CephFsDataRangeReader, CephFsFileDataContent, CephFsFileDataDescriptor,
    CephFsSparseExtentProof, SourceDbCephFsObjectReader,
};

pub(crate) struct CephFsFileReadRequest {
    pub(crate) data_source_id: DataSourceId,
    pub(crate) size: u64,
    pub(crate) filesystem_identity: String,
    pub(crate) filesystem_id: i64,
    pub(crate) fsmap_epoch: u32,
    pub(crate) inode: u64,
    pub(crate) stripe_unit: u32,
    pub(crate) stripe_count: u32,
    pub(crate) object_size: u32,
    pub(crate) pool_id: i64,
    pub(crate) pool_namespace: String,
    pub(crate) inline_data: Option<Vec<u8>>,
    pub(crate) sparse_extents: Vec<CephFsSparseExtentProof>,
    pub(crate) projection_sha256: String,
}

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
    request: &CephFsFileReadRequest,
) -> CephFsSourceResult<PreparedCephFsFileReader> {
    if request.data_source_id != *data_source_id {
        return Err(CephFsSourceError::InvalidInput(
            "file-read request is not bound to this CephFS source",
        ));
    }
    let runtime = load_runtime(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        request.pool_id,
    )?;
    if runtime.descriptor.identity != request.filesystem_identity
        || runtime.descriptor.filesystem_id != request.filesystem_id
        || runtime.descriptor.fsmap_epoch != request.fsmap_epoch
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
    if aggregate.lineage.namespace_projection_sha256 != request.projection_sha256 {
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
        request.stripe_unit,
        request.stripe_count,
        request.object_size,
        runtime.resolved_pool_id,
        request.pool_namespace.clone(),
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let data_descriptor = CephFsFileDataDescriptor::with_content(
        request.filesystem_identity.clone(),
        request.filesystem_id,
        request.fsmap_epoch,
        request.inode,
        request.size,
        layout,
        CephFsFileDataContent {
            inline_data: request.inline_data.clone(),
            sparse_extents: request.sparse_extents.clone(),
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
        size: request.size,
        projection_sha256: request.projection_sha256.clone(),
        lineage_fingerprint: runtime.lineage_fingerprint,
    })
}
