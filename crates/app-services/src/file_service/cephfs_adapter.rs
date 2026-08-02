use crate::{
    ceph_reconstruction::CephFsFileReadRequest,
    file_service::{FileServiceError, PreviewDescriptor},
};

pub(super) fn file_read_request(
    descriptor: &PreviewDescriptor,
) -> Result<CephFsFileReadRequest, FileServiceError> {
    if descriptor.source_kind != "ceph_fs" {
        return Err(FileServiceError::other(
            "File descriptor is not bound to a CephFS source",
        ));
    }
    let file = descriptor.ceph_fs.as_ref().ok_or_else(|| {
        FileServiceError::other("CephFS preview descriptor is missing its file locator")
    })?;
    Ok(CephFsFileReadRequest {
        data_source_id: domain::DataSourceId(descriptor.data_source_id.clone()),
        size: descriptor.size,
        filesystem_identity: file.filesystem_identity.clone(),
        filesystem_id: file.filesystem_id,
        fsmap_epoch: file.fsmap_epoch,
        inode: file.inode,
        stripe_unit: file.stripe_unit,
        stripe_count: file.stripe_count,
        object_size: file.object_size,
        pool_id: file.pool_id,
        pool_namespace: file.pool_namespace.clone(),
        inline_data: file.inline_data.clone(),
        sparse_extents: file.sparse_extents.clone(),
        projection_sha256: file.projection_sha256.clone(),
    })
}
