use super::{
    accumulator::ClosedScope,
    error::BlueStoreOmapError,
    types::{
        BlueStoreOmapOwner, BlueStoreOmapSnapshot, BlueStoreRbdDirectoryMapping, BlueStoreRbdHeader,
    },
};

pub(super) fn append_directory_mappings(
    snapshot: &mut BlueStoreOmapSnapshot,
    closed: &ClosedScope,
    owner: &BlueStoreOmapOwner,
) -> Result<(), BlueStoreOmapError> {
    for (image_name, image_id) in &closed.directory.name_to_id {
        let reverse = closed.directory.id_to_name.get(image_id);
        if reverse.is_some_and(|name| name != image_name) {
            return Err(BlueStoreOmapError::ConflictingDirectoryMapping {
                scope: closed.scope,
            });
        }
        snapshot
            .directory_mappings
            .push(BlueStoreRbdDirectoryMapping {
                scope: closed.scope,
                owner_nid: owner.nid,
                image_name: image_name.clone(),
                image_id: image_id.clone(),
                bidirectional: reverse.is_some(),
            });
    }
    for (image_id, image_name) in &closed.directory.id_to_name {
        if let Some(mapped_id) = closed.directory.name_to_id.get(image_name) {
            if mapped_id != image_id {
                return Err(BlueStoreOmapError::ConflictingDirectoryMapping {
                    scope: closed.scope,
                });
            }
        } else {
            snapshot
                .directory_mappings
                .push(BlueStoreRbdDirectoryMapping {
                    scope: closed.scope,
                    owner_nid: owner.nid,
                    image_name: image_name.clone(),
                    image_id: image_id.clone(),
                    bidirectional: false,
                });
        }
    }
    Ok(())
}

pub(super) fn append_header(
    snapshot: &mut BlueStoreOmapSnapshot,
    closed: &ClosedScope,
    owner: &BlueStoreOmapOwner,
    image_id: &str,
) -> Result<(), BlueStoreOmapError> {
    if snapshot
        .rbd_headers
        .iter()
        .any(|header| header.image_id == image_id)
    {
        return Err(BlueStoreOmapError::DuplicateRbdHeader {
            image_id: image_id.to_string(),
        });
    }
    snapshot.rbd_headers.push(BlueStoreRbdHeader {
        scope: closed.scope,
        owner_nid: owner.nid,
        image_id: image_id.to_string(),
        size: closed.header.size,
        order: closed.header.order,
        features: closed.header.features,
        object_prefix: closed.header.object_prefix.clone(),
        stripe_unit: closed.header.stripe_unit,
        stripe_count: closed.header.stripe_count,
        data_pool_id: closed.header.data_pool_id,
    });
    Ok(())
}
