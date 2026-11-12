use ceph_wire::{BlueStoreKeySpace, BlueStoreOmapMode, BlueStoreSuperRecord};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
};
use transport::CommandError;

#[derive(Default)]
pub(super) struct SuperAccumulator {
    pub(super) nid_max: Option<u64>,
    pub(super) blobid_max: Option<u64>,
    pub(super) min_alloc_size: Option<u64>,
    pub(super) ondisk_format: Option<i32>,
    pub(super) min_compat_ondisk_format: Option<i32>,
    pub(super) per_pool_omap: Option<String>,
    pub(super) freelist_type: Option<String>,
    pub(super) observed_count: u64,
    pub(super) deferred_count: u64,
}

impl SuperAccumulator {
    pub(super) fn observe(&mut self, record: BlueStoreSuperRecord) -> Result<(), CommandError> {
        increment(&mut self.observed_count)?;
        match record {
            BlueStoreSuperRecord::NidMax(value) => set_once(&mut self.nid_max, value, "nid_max"),
            BlueStoreSuperRecord::BlobIdMax(value) => {
                set_once(&mut self.blobid_max, value, "blobid_max")
            }
            BlueStoreSuperRecord::MinAllocSize(value) => {
                set_once(&mut self.min_alloc_size, value, "min_alloc_size")
            }
            BlueStoreSuperRecord::OndiskFormat(value) => {
                set_once(&mut self.ondisk_format, value, "ondisk_format")
            }
            BlueStoreSuperRecord::MinCompatOndiskFormat(value) => set_once(
                &mut self.min_compat_ondisk_format,
                value,
                "min_compat_ondisk_format",
            ),
            BlueStoreSuperRecord::PerPoolOmap(value) => set_once(
                &mut self.per_pool_omap,
                omap_mode(value).to_string(),
                "per_pool_omap",
            ),
            BlueStoreSuperRecord::FreelistType(value) => {
                set_once(&mut self.freelist_type, value, "freelist_type")
            }
            BlueStoreSuperRecord::Unknown { .. } => increment(&mut self.deferred_count),
        }
    }

    pub(super) fn merge(&mut self, other: Self) -> Result<(), CommandError> {
        merge_option(&mut self.nid_max, other.nid_max, "nid_max")?;
        merge_option(&mut self.blobid_max, other.blobid_max, "blobid_max")?;
        merge_option(
            &mut self.min_alloc_size,
            other.min_alloc_size,
            "min_alloc_size",
        )?;
        merge_option(
            &mut self.ondisk_format,
            other.ondisk_format,
            "ondisk_format",
        )?;
        merge_option(
            &mut self.min_compat_ondisk_format,
            other.min_compat_ondisk_format,
            "min_compat_ondisk_format",
        )?;
        merge_option(
            &mut self.per_pool_omap,
            other.per_pool_omap,
            "per_pool_omap",
        )?;
        merge_option(
            &mut self.freelist_type,
            other.freelist_type,
            "freelist_type",
        )?;
        self.observed_count = self
            .observed_count
            .checked_add(other.observed_count)
            .ok_or_else(|| semantic_error("super observed-count overflow"))?;
        self.deferred_count = self
            .deferred_count
            .checked_add(other.deferred_count)
            .ok_or_else(|| semantic_error("super deferred-count overflow"))?;
        Ok(())
    }

    pub(super) fn finish(self, inventory_id: &str) -> CephBluestoreSuperRecord {
        CephBluestoreSuperRecord {
            inventory_id: inventory_id.to_string(),
            nid_max: self.nid_max,
            blobid_max: self.blobid_max,
            min_alloc_size: self.min_alloc_size,
            ondisk_format: self.ondisk_format,
            min_compat_ondisk_format: self.min_compat_ondisk_format,
            per_pool_omap: self.per_pool_omap,
            freelist_type: self.freelist_type,
            observed_count: self.observed_count,
            deferred_count: self.deferred_count,
        }
    }
}

pub(super) struct SharedBlobRows {
    pub(super) record: CephBluestoreSharedBlobRecord,
    pub(super) refs: Vec<CephBluestoreSharedBlobRefRecord>,
}

pub(super) fn key_space_index(key_space: BlueStoreKeySpace) -> usize {
    match key_space {
        BlueStoreKeySpace::Super => 0,
        BlueStoreKeySpace::Collection => 1,
        BlueStoreKeySpace::Object => 2,
        BlueStoreKeySpace::SharedBlob => 3,
    }
}

fn omap_mode(value: BlueStoreOmapMode) -> &'static str {
    match value {
        BlueStoreOmapMode::Bulk => "bulk",
        BlueStoreOmapMode::PerPool => "perPool",
        BlueStoreOmapMode::PerPg => "perPg",
    }
}

fn set_once<T>(target: &mut Option<T>, value: T, field: &str) -> Result<(), CommandError> {
    if target.replace(value).is_some() {
        return Err(semantic_error(format!("duplicate super field {field}")));
    }
    Ok(())
}

fn merge_option<T>(
    target: &mut Option<T>,
    value: Option<T>,
    field: &str,
) -> Result<(), CommandError> {
    if let Some(value) = value {
        set_once(target, value, field)?;
    }
    Ok(())
}

pub(super) fn increment(value: &mut u64) -> Result<(), CommandError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| semantic_error("semantic count overflow"))?;
    Ok(())
}

fn semantic_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "BlueStore semantic recovery failed: {}",
        message.into()
    ))
}
