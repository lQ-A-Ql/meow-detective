use std::mem::size_of;

use crate::{
    bluestore_semantic::{
        denc::ensure_limit,
        types::{
            BlueStoreAttributeSummary, BlueStoreBlob, BlueStoreBlobUseRef,
            BlueStoreExtentShardDescriptor, BlueStoreLogicalExtent, BlueStorePhysicalExtent,
            BlueStoreSemanticLimits, BlueStoreSharedBlobExtentRef, BlueStoreZoneOffsetRef,
        },
    },
    error::{CephWireError, Result},
};

const VALIDATION_INDEX_BYTES_PER_ENTRY: usize = 64;

pub(crate) struct SemanticBudget {
    limits: BlueStoreSemanticLimits,
    physical_extents: usize,
    blobs: usize,
    checksum_bytes: usize,
    use_tracker_entries: usize,
    decoded_heap_bytes: usize,
    work_units: usize,
}

impl SemanticBudget {
    pub(crate) fn new(limits: BlueStoreSemanticLimits) -> Self {
        Self {
            limits,
            physical_extents: 0,
            blobs: 0,
            checksum_bytes: 0,
            use_tracker_entries: 0,
            decoded_heap_bytes: 0,
            work_units: 0,
        }
    }

    pub(crate) fn claim_input(&mut self, count: usize) -> Result<()> {
        self.claim_work(count)
    }

    pub(crate) fn claim_retained_bytes(&mut self, count: usize) -> Result<()> {
        self.claim_heap(count)
    }

    pub(crate) fn claim_attributes(&mut self, count: usize) -> Result<()> {
        self.claim_items::<BlueStoreAttributeSummary>(count)
    }

    pub(crate) fn claim_extent_shards(&mut self, count: usize) -> Result<()> {
        self.claim_items::<BlueStoreExtentShardDescriptor>(count)
    }

    pub(crate) fn claim_zone_refs(&mut self, count: usize) -> Result<()> {
        self.claim_items::<BlueStoreZoneOffsetRef>(count)
    }

    pub(crate) fn claim_extent_records(&mut self, count: usize) -> Result<()> {
        self.claim_items::<BlueStoreLogicalExtent>(count)
    }

    pub(crate) fn claim_physical_extents(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.physical_extents,
            count,
            self.limits.max_physical_extents,
            "BlueStore physical extents",
        )?;
        self.claim_items::<BlueStorePhysicalExtent>(count)
    }

    pub(crate) fn claim_blobs(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.blobs,
            count,
            self.limits.max_blobs,
            "BlueStore blobs",
        )?;
        self.claim_items::<BlueStoreBlob>(count)
    }

    pub(crate) fn claim_checksum_bytes(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.checksum_bytes,
            count,
            self.limits.max_checksum_bytes,
            "BlueStore checksum bytes",
        )?;
        self.claim_work(count)
    }

    pub(crate) fn claim_checksum_words(&mut self, count: usize) -> Result<()> {
        self.claim_items::<u64>(count)
    }

    pub(crate) fn claim_use_tracker_entries(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.use_tracker_entries,
            count,
            self.limits.max_use_tracker_entries,
            "BlueStore use tracker entries",
        )?;
        self.claim_items::<BlueStoreBlobUseRef>(count)
    }

    pub(crate) fn claim_shared_blob_refs(&mut self, count: usize) -> Result<()> {
        self.claim_items::<BlueStoreSharedBlobExtentRef>(count)
    }

    pub(crate) fn claim_validation_entries(&mut self, count: usize) -> Result<()> {
        let bytes = checked_product(
            count,
            VALIDATION_INDEX_BYTES_PER_ENTRY,
            "BlueStore validation index bytes",
        )?;
        self.claim_heap(bytes)?;
        self.claim_work(count)
    }

    fn claim_items<T>(&mut self, count: usize) -> Result<()> {
        let bytes = checked_product(count, size_of::<T>(), "BlueStore decoded heap bytes")?;
        self.claim_heap(bytes)?;
        self.claim_work(count)
    }

    fn claim_heap(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.decoded_heap_bytes,
            count,
            self.limits.max_decoded_heap_bytes,
            "BlueStore decoded heap bytes",
        )
    }

    fn claim_work(&mut self, count: usize) -> Result<()> {
        claim(
            &mut self.work_units,
            count,
            self.limits.max_decode_work_units,
            "BlueStore decode work units",
        )
    }
}

fn checked_product(count: usize, size: usize, context: &'static str) -> Result<usize> {
    count
        .checked_mul(size)
        .ok_or(CephWireError::LengthOverflow { context })
}

fn claim(used: &mut usize, count: usize, limit: usize, context: &'static str) -> Result<()> {
    let total = used
        .checked_add(count)
        .ok_or(CephWireError::LengthOverflow { context })?;
    ensure_limit(total, limit, context)?;
    *used = total;
    Ok(())
}
