use std::collections::{BTreeMap, BTreeSet};

use super::{invalid_extent, invalid_payload, missing_blob};
use crate::{
    bluestore_semantic::types::{
        BlueStoreBlob, BlueStoreBlobIdentity, BlueStoreExtentPayload, BlueStoreLogicalExtent,
    },
    error::{CephWireError, Result},
};

impl BlueStoreExtentPayload {
    pub fn validate_with_spanning_blobs(&self, spanning_blobs: &[BlueStoreBlob]) -> Result<()> {
        self.validate_with_optional_spanning_blobs(spanning_blobs, false)
            .map(|_| ())
    }

    pub(crate) fn validate_with_optional_spanning_blobs(
        &self,
        spanning_blobs: &[BlueStoreBlob],
        allow_missing_spanning: bool,
    ) -> Result<bool> {
        let spanning = index_blobs(spanning_blobs, true)?;
        let local = index_blobs(&self.blobs, false)?;
        let mut defined_local = BTreeSet::new();
        let mut previous_end = None;
        let mut missing_spanning = false;
        for extent in &self.extents {
            validate_extent_order(extent, previous_end)?;
            match resolve_blob(
                extent,
                &local,
                &spanning,
                &defined_local,
                allow_missing_spanning,
            )? {
                Some(blob) => {
                    validate_blob_range(extent, blob.logical_length)?;
                    validate_blob_allocation(extent, blob)?;
                }
                None => missing_spanning = true,
            }
            if extent.defines_blob {
                validate_blob_definition(extent, &mut defined_local)?;
            }
            previous_end = Some(u64::from(extent.logical_offset) + u64::from(extent.length));
        }
        if defined_local.len() != local.len() {
            return Err(invalid_payload(
                "not every local blob has an inline definition",
            ));
        }
        Ok(missing_spanning)
    }
}

fn index_blobs(blobs: &[BlueStoreBlob], spanning: bool) -> Result<BTreeMap<u64, &BlueStoreBlob>> {
    let mut indexed = BTreeMap::new();
    for blob in blobs {
        let (kind, id) = match blob.identity {
            BlueStoreBlobIdentity::Spanning(id) if spanning => ("spanning", id),
            BlueStoreBlobIdentity::Local(id) if !spanning => ("local", u64::from(id)),
            _ => return Err(invalid_payload("blob identity is in the wrong storage set")),
        };
        if indexed.insert(id, blob).is_some() {
            return Err(CephWireError::DuplicateBlueStoreBlob { kind, blob_id: id });
        }
    }
    Ok(indexed)
}

fn resolve_blob<'a>(
    extent: &BlueStoreLogicalExtent,
    local: &'a BTreeMap<u64, &BlueStoreBlob>,
    spanning: &'a BTreeMap<u64, &BlueStoreBlob>,
    defined_local: &BTreeSet<u32>,
    allow_missing_spanning: bool,
) -> Result<Option<&'a BlueStoreBlob>> {
    match extent.blob {
        BlueStoreBlobIdentity::Spanning(id) => match spanning.get(&id).copied() {
            Some(blob) => Ok(Some(blob)),
            None if allow_missing_spanning => Ok(None),
            None => Err(missing_blob(extent.record_index, "spanning", id)),
        },
        BlueStoreBlobIdentity::Local(id) => {
            if !extent.defines_blob && !defined_local.contains(&id) {
                return Err(missing_blob(extent.record_index, "local", u64::from(id)));
            }
            local
                .get(&u64::from(id))
                .copied()
                .map(Some)
                .ok_or_else(|| missing_blob(extent.record_index, "local", u64::from(id)))
        }
    }
}

fn validate_extent_order(extent: &BlueStoreLogicalExtent, previous_end: Option<u64>) -> Result<()> {
    if extent.length == 0 {
        return Err(invalid_extent(
            extent.record_index,
            "length must be non-zero",
        ));
    }
    if let Some(previous_end) = previous_end {
        if u64::from(extent.logical_offset) < previous_end {
            return Err(CephWireError::BlueStoreLogicalExtentOverlap {
                previous_end,
                logical_offset: u64::from(extent.logical_offset),
            });
        }
    }
    u64::from(extent.logical_offset)
        .checked_add(u64::from(extent.length))
        .ok_or(CephWireError::IntegerOverflow {
            context: "BlueStore logical extent end",
        })?;
    Ok(())
}

fn validate_blob_range(extent: &BlueStoreLogicalExtent, logical_length: u32) -> Result<()> {
    let end = u64::from(extent.blob_offset) + u64::from(extent.length);
    if end > u64::from(logical_length) {
        return Err(CephWireError::BlueStoreBlobRangeOverflow {
            record_index: extent.record_index,
            blob_offset: extent.blob_offset,
            length: extent.length,
            logical_length,
        });
    }
    Ok(())
}

fn validate_blob_allocation(extent: &BlueStoreLogicalExtent, blob: &BlueStoreBlob) -> Result<()> {
    if blob.flags.compressed {
        if blob.physical_extents.is_empty()
            || blob
                .physical_extents
                .iter()
                .any(|physical| physical.offset.is_none())
        {
            return Err(invalid_extent(
                extent.record_index,
                "compressed blob is not fully allocated",
            ));
        }
        return Ok(());
    }
    let mut remaining_offset = u64::from(extent.blob_offset);
    let mut remaining_length = u64::from(extent.length);
    for physical in &blob.physical_extents {
        let physical_length = u64::from(physical.length);
        if remaining_offset >= physical_length {
            remaining_offset -= physical_length;
            continue;
        }
        let covered = (physical_length - remaining_offset).min(remaining_length);
        if physical.offset.is_none() {
            return Err(invalid_extent(
                extent.record_index,
                "logical extent references an unallocated blob range",
            ));
        }
        remaining_length -= covered;
        remaining_offset = 0;
        if remaining_length == 0 {
            return Ok(());
        }
    }
    Err(invalid_extent(
        extent.record_index,
        "logical extent is not covered by the blob physical map",
    ))
}

fn validate_blob_definition(
    extent: &BlueStoreLogicalExtent,
    defined_local: &mut BTreeSet<u32>,
) -> Result<()> {
    let BlueStoreBlobIdentity::Local(id) = extent.blob else {
        return Err(invalid_extent(
            extent.record_index,
            "spanning extents cannot define inline blobs",
        ));
    };
    if id != extent.record_index {
        return Err(invalid_extent(
            extent.record_index,
            "inline blob id must equal the defining extent index",
        ));
    }
    if !defined_local.insert(id) {
        return Err(CephWireError::DuplicateBlueStoreBlob {
            kind: "local",
            blob_id: u64::from(id),
        });
    }
    Ok(())
}
