use std::collections::HashMap;

use ceph_wire::{classify_cephfs_metadata_object_name, CephFsMetadataObjectClass};
use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::{
        CephBluestoreObjectInventoryEntry, CephBluestoreObjectPageCursor, CephBluestoreSemanticRepo,
    },
    ceph_fs_metadata_inventory_repo::{
        cephfs_metadata_inventory_sha256, CephFsMetadataInventory, CephFsMetadataInventoryManifest,
        CephFsMetadataInventoryRepo, CephFsMetadataInventoryRepoError,
        CephFsMetadataObjectProjection, CEPHFS_METADATA_CLASSIFIER_PROFILE,
        CEPHFS_METADATA_SCHEMA_VERSION,
    },
};
use thiserror::Error;

use super::{inventory_digest, CephFsDescriptor, CephFsObjectLocator};

const INVENTORY_PAGE_SIZE: u32 = 1024;
pub const CEPHFS_HEAD_SNAP_HEX: &str = "fffffffffffffffe";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsInventoryError {
    #[error("invalid CephFS descriptor or source binding: {0}")]
    InvalidBinding(&'static str),
    #[error("invalid CephFS object locator")]
    InvalidLocator,
    #[error("CephFS metadata object crosses pool {observed_pool}; expected {expected_pool}")]
    CrossPoolReference {
        expected_pool: i64,
        observed_pool: i64,
    },
    #[error("CephFS metadata object is not a live head object: {object_identity}")]
    UnsupportedSnapshot { object_identity: String },
    #[error("CephFS object locator resolves to conflicting identities: {locator}")]
    ObjectIdentityConflict { locator: String },
    #[error("CephFS source snapshot changed while inventory pages were read")]
    SourceSnapshotConflict,
    #[error("CephFS source inventory is non-deterministic")]
    DeterminismConflict,
    #[error("CephFS object range overflows: {locator}")]
    RangeOverflow { locator: String },
    #[error("CephFS object range exceeds size {object_size}: {locator}")]
    RangeOutOfBounds { locator: String, object_size: u64 },
    #[error("CephFS metadata inventory database operation failed")]
    Database,
}

impl transport::ServiceErrorCategory for CephFsInventoryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidBinding(_) | Self::InvalidLocator | Self::RangeOutOfBounds { .. } => {
                transport::ErrorCategory::Validation
            }
            Self::Database => transport::ErrorCategory::Io,
            Self::RangeOverflow { .. }
            | Self::CrossPoolReference { .. }
            | Self::UnsupportedSnapshot { .. }
            | Self::ObjectIdentityConflict { .. }
            | Self::SourceSnapshotConflict
            | Self::DeterminismConflict => transport::ErrorCategory::Parser,
        }
    }
}

pub fn inventory_cephfs_metadata_pool(
    conn: &rusqlite::Connection,
    descriptor: &CephFsDescriptor,
    data_source_id: &str,
    bluestore_inventory_id: &str,
) -> Result<CephFsMetadataInventory, CephFsInventoryError> {
    validate_source_binding(descriptor, data_source_id, bluestore_inventory_id)?;
    let read_transaction = conn
        .unchecked_transaction()
        .map_err(|_| CephFsInventoryError::Database)?;
    let (semantic_sha256, objects) =
        load_projections(&read_transaction, descriptor, bluestore_inventory_id)?;
    read_transaction
        .commit()
        .map_err(|_| CephFsInventoryError::Database)?;
    persist_inventory(
        conn,
        descriptor,
        data_source_id,
        bluestore_inventory_id,
        semantic_sha256,
        objects,
    )
}

fn load_projections(
    conn: &rusqlite::Connection,
    descriptor: &CephFsDescriptor,
    bluestore_inventory_id: &str,
) -> Result<(String, Vec<CephFsMetadataObjectProjection>), CephFsInventoryError> {
    let semantic_repo = CephBluestoreSemanticRepo::new(conn);
    let mut cursor = None;
    let mut semantic_sha256 = None;
    let mut objects = Vec::new();
    let mut locators = HashMap::new();
    loop {
        let page = semantic_repo
            .list_objects_by_pool_after(
                bluestore_inventory_id,
                descriptor.metadata_pool.pool_id,
                cursor.as_ref(),
                INVENTORY_PAGE_SIZE,
            )
            .map_err(|_| CephFsInventoryError::Database)?;
        validate_page_snapshot(&mut semantic_sha256, &page.semantic_sha256)?;
        for object in page.objects {
            let projection = project_object(descriptor, &object)?;
            if let Some(previous) = locators.insert(
                projection.locator.clone(),
                projection.object_identity_sha256.clone(),
            ) {
                if previous != projection.object_identity_sha256 {
                    return Err(CephFsInventoryError::ObjectIdentityConflict {
                        locator: projection.locator,
                    });
                }
                continue;
            }
            objects.push(projection);
        }
        let Some(next) = page.next_cursor else {
            break;
        };
        cursor = Some(
            CephBluestoreObjectPageCursor::new(next.as_str().to_string())
                .map_err(|_| CephFsInventoryError::SourceSnapshotConflict)?,
        );
    }
    semantic_sha256
        .map(|semantic_sha256| (semantic_sha256, objects))
        .ok_or(CephFsInventoryError::SourceSnapshotConflict)
}

fn persist_inventory(
    conn: &rusqlite::Connection,
    descriptor: &CephFsDescriptor,
    data_source_id: &str,
    bluestore_inventory_id: &str,
    semantic_sha256: String,
    objects: Vec<CephFsMetadataObjectProjection>,
) -> Result<CephFsMetadataInventory, CephFsInventoryError> {
    let unknown_object_count = objects
        .iter()
        .filter(|object| object.classification_state == "metadata_only")
        .count() as u64;
    let mut manifest = CephFsMetadataInventoryManifest {
        filesystem_identity: descriptor.identity.clone(),
        inventory_id: bluestore_inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        filesystem_id: descriptor.filesystem_id,
        fsmap_epoch: descriptor.fsmap_epoch,
        metadata_pool_id: descriptor.metadata_pool.pool_id,
        schema_version: CEPHFS_METADATA_SCHEMA_VERSION,
        classifier_profile: CEPHFS_METADATA_CLASSIFIER_PROFILE.to_string(),
        source_semantic_sha256: semantic_sha256,
        inventory_sha256: String::new(),
        object_count: objects.len() as u64,
        unknown_object_count,
        complete: true,
    };
    manifest.inventory_sha256 = cephfs_metadata_inventory_sha256(&manifest, &objects);
    let inventory = CephFsMetadataInventory { manifest, objects };
    CephFsMetadataInventoryRepo::new(conn)
        .replace(&inventory)
        .map_err(map_repo_error)?;
    Ok(inventory)
}

fn project_object(
    descriptor: &CephFsDescriptor,
    object: &CephBluestoreObjectInventoryEntry,
) -> Result<CephFsMetadataObjectProjection, CephFsInventoryError> {
    let expected_pool = descriptor.metadata_pool.pool_id;
    if object.decoded_pool != expected_pool {
        return Err(CephFsInventoryError::CrossPoolReference {
            expected_pool,
            observed_pool: object.decoded_pool,
        });
    }
    if object.snap_hex != CEPHFS_HEAD_SNAP_HEX {
        return Err(CephFsInventoryError::UnsupportedSnapshot {
            object_identity: object.object_identity_sha256.clone(),
        });
    }
    let locator = CephFsObjectLocator::new(
        descriptor.filesystem_id,
        expected_pool,
        object.object_namespace.clone(),
        object.object_name.clone(),
        descriptor.fsmap_epoch,
    )?
    .canonical();
    let (class, candidates) = classify_cephfs_metadata_object_name(&object.object_name);
    let (rule, candidate_mask, known) = classification(class, candidates.mask(), object);
    let classification_state = if !known {
        "metadata_only"
    } else if candidate_mask == 0 {
        "classified"
    } else {
        "candidate"
    };
    Ok(CephFsMetadataObjectProjection {
        record_sha256: inventory_digest::object_record_sha256(
            &locator,
            rule,
            candidate_mask,
            object,
        ),
        object_identity_sha256: object.object_identity_sha256.clone(),
        locator,
        candidate_mask,
        classification_state: classification_state.to_string(),
        classifier_rule: rule.to_string(),
    })
}

fn classification(
    class: CephFsMetadataObjectClass,
    candidate_mask: u8,
    object: &CephBluestoreObjectInventoryEntry,
) -> (&'static str, u8, bool) {
    if object.decode_status != "parsed" || object.deferred_reason.is_some() {
        return ("deferred_object", 0, false);
    }
    let rule = match class {
        CephFsMetadataObjectClass::DirFragmentCandidate { .. } => "dirfrag_candidate",
        CephFsMetadataObjectClass::StandaloneInodeCandidate { .. } => "inode_candidate",
        CephFsMetadataObjectClass::JournalData { .. } => "journal_data",
        CephFsMetadataObjectClass::JournalPointer { .. } => "journal_pointer",
        CephFsMetadataObjectClass::PurgeQueue { .. } => "purge_queue",
        CephFsMetadataObjectClass::SnapTable => "snap_table",
        CephFsMetadataObjectClass::AnchorTable => "anchor_table",
        CephFsMetadataObjectClass::RankTable { .. } => "rank_table",
        CephFsMetadataObjectClass::OpenFileTable { .. } => "open_file_table",
        CephFsMetadataObjectClass::Unknown => return ("unknown_object", 0, false),
    };
    (rule, candidate_mask, true)
}

fn validate_source_binding(
    descriptor: &CephFsDescriptor,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<(), CephFsInventoryError> {
    if descriptor.identity.trim().is_empty()
        || descriptor.filesystem_id < 0
        || descriptor.fsmap_epoch == 0
        || descriptor.metadata_pool.pool_id < 0
        || data_source_id.trim().is_empty()
        || inventory_id.trim().is_empty()
    {
        return Err(CephFsInventoryError::InvalidBinding(
            "identity, pool, or epoch is invalid",
        ));
    }
    let bound = descriptor
        .metadata_pool
        .provenance
        .iter()
        .any(|provenance| {
            provenance.source_identity == data_source_id
                && provenance.inventory_identity == inventory_id
        });
    if !bound {
        return Err(CephFsInventoryError::InvalidBinding(
            "source is not bound to the metadata pool",
        ));
    }
    Ok(())
}

fn validate_page_snapshot(
    expected: &mut Option<String>,
    observed: &str,
) -> Result<(), CephFsInventoryError> {
    if expected
        .replace(observed.to_string())
        .is_some_and(|expected| expected != observed)
    {
        return Err(CephFsInventoryError::SourceSnapshotConflict);
    }
    Ok(())
}

fn map_repo_error(error: CephFsMetadataInventoryRepoError) -> CephFsInventoryError {
    match error {
        CephFsMetadataInventoryRepoError::DeterminismConflict => {
            CephFsInventoryError::DeterminismConflict
        }
        CephFsMetadataInventoryRepoError::CrossPoolReference => CephFsInventoryError::Database,
        CephFsMetadataInventoryRepoError::Invalid(_)
        | CephFsMetadataInventoryRepoError::Database(_) => CephFsInventoryError::Database,
    }
}
