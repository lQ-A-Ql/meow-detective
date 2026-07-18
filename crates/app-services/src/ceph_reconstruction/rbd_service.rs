use std::collections::BTreeMap;
use std::collections::HashSet;

use persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreOmapRepo;
use thiserror::Error;

use super::{
    detect_rbd_image_filesystem, discover_rbd_images, RadosReplicaSource, RbdImageDescriptor,
    SourceDbRadosObjectProvider,
};

#[derive(Debug, Error)]
pub enum RbdReconstructionError {
    #[error("RBD replica coverage is not closed: expected {expected}, provided {provided}")]
    ReplicaCoverageNotClosed { expected: usize, provided: usize },
    #[error("duplicate RBD replica inventory binding: {inventory_id}")]
    DuplicateReplicaInventory { inventory_id: String },
    #[error("duplicate RBD replica data source binding: {data_source_id}")]
    DuplicateReplicaSource { data_source_id: String },
    #[error("source database could not be opened for inventory {inventory_id}: {detail}")]
    SourceDb {
        inventory_id: String,
        detail: String,
    },
    #[error("RBD OMAP metadata is missing for inventory {inventory_id}")]
    MissingOmap { inventory_id: String },
    #[error("RBD catalog failed for inventory {inventory_id}: {detail}")]
    Catalog {
        inventory_id: String,
        detail: String,
    },
    #[error("RBD image metadata conflicts across source replicas: {image_id}")]
    MetadataConflict { image_id: String },
    #[error("RBD image was not found: {image_id}")]
    ImageNotFound { image_id: String },
    #[error("RBD object provider could not be created: {detail}")]
    Provider { detail: String },
    #[error("RBD image filesystem detection failed for {image_id}: {detail}")]
    Filesystem { image_id: String, detail: String },
}

pub fn discover_rbd_images_from_source_dbs(
    replicas: &[RadosReplicaSource],
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
    validate_replica_set(replicas, replicas.len())?;
    let mut images = BTreeMap::new();
    for replica in replicas {
        let connection = persistence_sqlite::open_existing_source_read_only(
            &replica.source_db_path,
        )
        .map_err(|error| RbdReconstructionError::SourceDb {
            inventory_id: replica.inventory_id.clone(),
            detail: source_db_error_detail(&error),
        })?;
        let aggregate = CephBluestoreOmapRepo::new(&connection)
            .find_aggregate(&replica.inventory_id)
            .map_err(|error| RbdReconstructionError::SourceDb {
                inventory_id: replica.inventory_id.clone(),
                detail: format!("OMAP query failed: {}", source_db_error_detail(&error)),
            })?
            .ok_or_else(|| RbdReconstructionError::MissingOmap {
                inventory_id: replica.inventory_id.clone(),
            })?;
        let descriptors =
            discover_rbd_images(&aggregate).map_err(|error| RbdReconstructionError::Catalog {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?;
        for descriptor in descriptors {
            merge_descriptor(&mut images, descriptor)?;
        }
    }
    Ok(images.into_values().collect())
}

fn source_db_error_detail(error: &persistence_sqlite::DbError) -> String {
    match error {
        persistence_sqlite::DbError::Sqlite(error) => format!("sqlite error: {error}"),
        persistence_sqlite::DbError::Io(error) => {
            format!("io error of kind {:?}", error.kind())
        }
        persistence_sqlite::DbError::Migration(error) => format!("migration error: {error}"),
        persistence_sqlite::DbError::System(error) => format!("system error: {error}"),
    }
}

pub fn detect_rbd_image_from_source_dbs(
    replicas: Vec<RadosReplicaSource>,
    image_id: &str,
    expected_replica_count: usize,
) -> Result<crate::datasource_service::ImageFilesystemProbe, RbdReconstructionError> {
    validate_replica_set(&replicas, expected_replica_count)?;
    let descriptors = discover_rbd_images_from_source_dbs(&replicas)?;
    let descriptor = descriptors
        .into_iter()
        .find(|descriptor| descriptor.metadata.id == image_id)
        .ok_or_else(|| RbdReconstructionError::ImageNotFound {
            image_id: image_id.to_string(),
        })?;
    let provider = SourceDbRadosObjectProvider::new(
        replicas,
        descriptor.metadata.data_pool_id,
        Vec::new(),
        expected_replica_count,
    )
    .map_err(|error| RbdReconstructionError::Provider {
        detail: error.to_string(),
    })?;
    detect_rbd_image_filesystem(&descriptor, Box::new(provider)).map_err(|error| {
        RbdReconstructionError::Filesystem {
            image_id: image_id.to_string(),
            detail: error.to_string(),
        }
    })
}

fn validate_replica_set(
    replicas: &[RadosReplicaSource],
    expected_replica_count: usize,
) -> Result<(), RbdReconstructionError> {
    if replicas.is_empty()
        || expected_replica_count == 0
        || replicas.len() != expected_replica_count
    {
        return Err(RbdReconstructionError::ReplicaCoverageNotClosed {
            expected: expected_replica_count,
            provided: replicas.len(),
        });
    }
    let mut inventories = HashSet::with_capacity(replicas.len());
    let mut data_sources = HashSet::with_capacity(replicas.len());
    for replica in replicas {
        if !inventories.insert(replica.inventory_id.as_str()) {
            return Err(RbdReconstructionError::DuplicateReplicaInventory {
                inventory_id: replica.inventory_id.clone(),
            });
        }
        if !data_sources.insert(replica.data_source_id.0.as_str()) {
            return Err(RbdReconstructionError::DuplicateReplicaSource {
                data_source_id: replica.data_source_id.0.clone(),
            });
        }
    }
    Ok(())
}

fn merge_descriptor(
    images: &mut BTreeMap<String, RbdImageDescriptor>,
    descriptor: RbdImageDescriptor,
) -> Result<(), RbdReconstructionError> {
    let image_id = descriptor.metadata.id.clone();
    if let Some(existing) = images.get(&image_id) {
        if existing.metadata != descriptor.metadata || existing.context != descriptor.context {
            return Err(RbdReconstructionError::MetadataConflict { image_id });
        }
    } else {
        images.insert(image_id, descriptor);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rbd_service.rs"]
mod tests;
