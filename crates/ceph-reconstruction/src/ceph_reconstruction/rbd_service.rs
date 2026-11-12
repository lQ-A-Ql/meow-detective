use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;

use persistence_source::fjall::{ParserFieldQuery, ScanDirection};
use persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreOmapRepo;
use thiserror::Error;

use super::{
    assess_inventory_coverage, detect_rbd_image_filesystem, discover_rbd_images,
    RadosReplicaSource, RbdImageDescriptor, RbdReplicaPolicy, SourceDbRadosObjectProvider,
};

#[derive(Debug, Error)]
pub enum RbdReconstructionError {
    #[error("RBD replica coverage is not closed: expected {expected}, provided {provided}")]
    ReplicaCoverageNotClosed { expected: usize, provided: usize },
    #[error("duplicate RBD replica inventory binding: {inventory_id}")]
    DuplicateReplicaInventory { inventory_id: String },
    #[error("duplicate RBD replica data source binding: {data_source_id}")]
    DuplicateReplicaSource { data_source_id: String },
    #[error("RBD inventory identity conflict: {detail}")]
    IdentityConflict { detail: String },
    #[error("RBD replica coverage is not proven: {detail}")]
    CoverageNotProven { detail: String },
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
    discover_rbd_images_from_source_dbs_with_policy(replicas, &RbdReplicaPolicy::strict_legacy())
}

/// Discovers RBD descriptors for an active case. The source-local OMAP
/// aggregate remains the reference for scope and directory relationships;
/// RBD headers are read through the Fjall parser projection first.
pub fn discover_rbd_images_from_case_storage(
    case_root: &Path,
    replicas: &[RadosReplicaSource],
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
    validate_replica_bindings(replicas, None)?;
    discover_rbd_descriptors_with_case_storage(case_root, replicas)
}

/// Reads one RBD header from the Fjall parser projection when it is available.
/// The source database remains the reference fallback for sources created before
/// the projection was published. The bounded page prevents a malformed source
/// from turning a point lookup into an unbounded scan.
pub fn find_rbd_header_from_case_storage(
    case_root: &Path,
    replica: &RadosReplicaSource,
    image_id: &str,
) -> Result<
    Option<
        persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreRbdHeaderRecord,
    >,
    RbdReconstructionError,
> {
    if image_id.is_empty() || image_id.contains('\0') {
        return Err(RbdReconstructionError::Catalog {
            inventory_id: replica.inventory_id.clone(),
            detail: "RBD image identity is invalid".to_owned(),
        });
    }
    const HEADER_KIND: &str = "ceph_bluestore_rbd_headers";
    const MAX_HEADERS: usize = 4096;
    if let Ok(source) = source_kernel::source_db::open_registered_fjall_source_read_only(
        case_root,
        &replica.data_source_id,
    ) {
        let snapshot = source.snapshot();
        if source
            .source_projection_complete(&snapshot, &replica.data_source_id.0)
            .map_err(|error| RbdReconstructionError::Catalog {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?
        {
            let page = source
                .parser_list_by_field(
                    &snapshot,
                    ParserFieldQuery {
                        source_id: &replica.data_source_id.0,
                        kind: HEADER_KIND,
                        field: "image_id",
                        value: image_id,
                    },
                    None,
                    ScanDirection::Forward,
                    MAX_HEADERS,
                )
                .map_err(|error| RbdReconstructionError::Catalog {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                })?;
            if page.records.len() > MAX_HEADERS {
                return Err(RbdReconstructionError::Catalog {
                    inventory_id: replica.inventory_id.clone(),
                    detail: "RBD header projection exceeds the bounded query limit".to_owned(),
                });
            }
            for record in page.records {
                if let Some(header) =
                    header_from_parser_record(&record, &replica.inventory_id, image_id)?
                {
                    return Ok(Some(header));
                }
            }
        }
    }
    let connection = persistence_sqlite::open_existing_source_read_only(&replica.source_db_path)
        .map_err(|error| RbdReconstructionError::SourceDb {
            inventory_id: replica.inventory_id.clone(),
            detail: source_db_error_detail(&error),
        })?;
    CephBluestoreOmapRepo::new(&connection)
        .find_rbd_header(&replica.inventory_id, image_id)
        .map_err(|error| RbdReconstructionError::SourceDb {
            inventory_id: replica.inventory_id.clone(),
            detail: source_db_error_detail(&error),
        })
}

fn header_from_parser_record(
    record: &persistence_source::fjall::ParserRecord,
    inventory_id: &str,
    requested_image_id: &str,
) -> Result<
    Option<
        persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreRbdHeaderRecord,
    >,
    RbdReconstructionError,
> {
    let payload = &record.payload;
    let payload_inventory = payload
        .get("inventory_id")
        .and_then(serde_json::Value::as_str);
    if payload_inventory != Some(inventory_id) {
        return Ok(None);
    }
    let Some(image_id) = payload.get("image_id").and_then(serde_json::Value::as_str) else {
        return Err(RbdReconstructionError::Catalog {
            inventory_id: inventory_id.to_owned(),
            detail: "RBD header projection is missing image identity".to_owned(),
        });
    };
    if image_id != requested_image_id {
        return Ok(None);
    }
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let integer = |key: &str| payload.get(key).and_then(serde_json::Value::as_i64);
    let boolean = |key: &str| {
        payload
            .get(key)
            .and_then(|value| value.as_bool().or_else(|| value.as_i64().map(|v| v != 0)))
            .unwrap_or(false)
    };
    Ok(Some(
        persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreRbdHeaderRecord {
            inventory_id: inventory_id.to_owned(),
            scope_identity: text("scope_identity").unwrap_or_default(),
            owner_nid_hex: text("owner_nid_hex").unwrap_or_default(),
            image_id: image_id.to_owned(),
            size_hex: text("size_hex"),
            object_order: integer("object_order").and_then(|value| u8::try_from(value).ok()),
            features_hex: text("features_hex"),
            operation_features_hex: text("operation_features_hex"),
            parent_key_present: boolean("parent_key_present"),
            object_prefix: text("object_prefix"),
            stripe_unit_hex: text("stripe_unit_hex"),
            stripe_count_hex: text("stripe_count_hex"),
            data_pool_id: integer("data_pool_id"),
        },
    ))
}

pub fn discover_rbd_images_from_source_dbs_unbound(
    replicas: &[RadosReplicaSource],
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
    validate_replica_bindings(replicas, None)?;
    discover_rbd_descriptors(replicas)
}

pub fn discover_rbd_images_from_source_dbs_with_policy(
    replicas: &[RadosReplicaSource],
    policy: &RbdReplicaPolicy,
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
    validate_replica_set(replicas, policy)?;
    discover_rbd_descriptors(replicas)
}

fn discover_rbd_descriptors(
    replicas: &[RadosReplicaSource],
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
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

fn discover_rbd_descriptors_with_case_storage(
    case_root: &Path,
    replicas: &[RadosReplicaSource],
) -> Result<Vec<RbdImageDescriptor>, RbdReconstructionError> {
    let mut images = BTreeMap::new();
    for replica in replicas {
        let connection = persistence_sqlite::open_existing_source_read_only(
            &replica.source_db_path,
        )
        .map_err(|error| RbdReconstructionError::SourceDb {
            inventory_id: replica.inventory_id.clone(),
            detail: source_db_error_detail(&error),
        })?;
        let mut aggregate = CephBluestoreOmapRepo::new(&connection)
            .find_aggregate(&replica.inventory_id)
            .map_err(|error| RbdReconstructionError::SourceDb {
                inventory_id: replica.inventory_id.clone(),
                detail: format!("OMAP query failed: {}", source_db_error_detail(&error)),
            })?
            .ok_or_else(|| RbdReconstructionError::MissingOmap {
                inventory_id: replica.inventory_id.clone(),
            })?;
        for header in &mut aggregate.rbd_headers {
            if let Some(projected) =
                find_rbd_header_from_case_storage(case_root, replica, &header.image_id)?
            {
                *header = projected;
            }
        }
        for descriptor in
            discover_rbd_images(&aggregate).map_err(|error| RbdReconstructionError::Catalog {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?
        {
            merge_descriptor(&mut images, descriptor)?;
        }
    }
    Ok(images.into_values().collect())
}

fn source_db_error_detail(error: &persistence_sqlite::DbError) -> String {
    match error {
        persistence_sqlite::DbError::ReadCancelled => "read cancelled".into(),
        persistence_sqlite::DbError::ReadTimedOut => "read timed out".into(),
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
) -> Result<source_kernel::partition_probe::ImageFilesystemProbe, RbdReconstructionError> {
    detect_rbd_image_from_source_dbs_with_policy(
        replicas,
        image_id,
        &RbdReplicaPolicy::strict_legacy(),
    )
}

pub fn detect_rbd_image_from_source_dbs_with_policy(
    replicas: Vec<RadosReplicaSource>,
    image_id: &str,
    policy: &RbdReplicaPolicy,
) -> Result<source_kernel::partition_probe::ImageFilesystemProbe, RbdReconstructionError> {
    validate_replica_set(&replicas, policy)?;
    let descriptors = discover_rbd_images_from_source_dbs_with_policy(&replicas, policy)?;
    let descriptor = descriptors
        .into_iter()
        .find(|descriptor| descriptor.metadata.id == image_id)
        .ok_or_else(|| RbdReconstructionError::ImageNotFound {
            image_id: image_id.to_string(),
        })?;
    let provider = SourceDbRadosObjectProvider::new_with_policy(
        replicas,
        descriptor.metadata.data_pool_id,
        Vec::new(),
        policy.clone(),
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
    policy: &RbdReplicaPolicy,
) -> Result<(), RbdReconstructionError> {
    let expected_count = policy.expected_count();
    if replicas.len() != expected_count {
        return Err(RbdReconstructionError::ReplicaCoverageNotClosed {
            expected: expected_count,
            provided: replicas.len(),
        });
    }
    validate_replica_bindings(replicas, Some(policy))
}

fn validate_replica_bindings(
    replicas: &[RadosReplicaSource],
    policy: Option<&RbdReplicaPolicy>,
) -> Result<(), RbdReconstructionError> {
    let mut inventories = HashSet::with_capacity(replicas.len());
    let mut data_sources = HashSet::with_capacity(replicas.len());
    if replicas.is_empty() {
        return Err(RbdReconstructionError::CoverageNotProven {
            detail: "no RBD replica bindings were supplied".to_string(),
        });
    }
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
    if let Some(policy) = policy {
        let evidence = replicas
            .iter()
            .map(|replica| super::InventoryEvidence {
                source_id: replica.data_source_id.0.clone(),
                inventory_id: replica.inventory_id.clone(),
                identity: replica.identity.clone(),
            })
            .collect::<Vec<_>>();
        let coverage = assess_inventory_coverage(&evidence, policy);
        if coverage.is_conflicted() {
            return Err(RbdReconstructionError::IdentityConflict {
                detail: coverage.diagnostics.join("; "),
            });
        }
        if !coverage.is_complete() {
            return Err(RbdReconstructionError::CoverageNotProven {
                detail: coverage.diagnostics.join("; "),
            });
        }
    } else {
        let mut osd_ids = HashSet::with_capacity(replicas.len());
        let mut fsids = HashSet::with_capacity(replicas.len());
        for replica in replicas {
            if !replica.identity.is_complete() {
                return Err(RbdReconstructionError::CoverageNotProven {
                    detail: "one or more inventory entries lack complete OSD identity".to_string(),
                });
            }
            if !osd_ids.insert(replica.identity.osd_id) {
                return Err(RbdReconstructionError::IdentityConflict {
                    detail: "OSD identities are duplicated".to_string(),
                });
            }
            if let Some(fsid) = replica.identity.ceph_fsid.as_deref() {
                fsids.insert(fsid);
            }
        }
        if fsids.len() > 1 {
            return Err(RbdReconstructionError::IdentityConflict {
                detail: "Ceph FSIDs conflict across inventory entries".to_string(),
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
