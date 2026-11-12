use std::collections::BTreeSet;

use super::{
    CephFsObjectMetadata, CephFsObjectRange, CephFsObjectReadError, CephFsObjectReadProvenance,
};
use crate::ceph_reconstruction::cephfs::{CephFsDescriptor, CephFsObjectLocator};

const MAX_RESPONSE_PROVENANCE: usize = 64;

pub(in crate::ceph_reconstruction::cephfs) fn validate_metadata_response(
    descriptor: &CephFsDescriptor,
    locator: &CephFsObjectLocator,
    metadata: &CephFsObjectMetadata,
) -> Result<(), CephFsObjectReadError> {
    let canonical = locator.canonical();
    if metadata.filesystem_identity != descriptor.identity
        || metadata.locator != canonical
        || metadata.object_size == 0
        || !valid_provenance(&metadata.provenance)
    {
        return Err(response_mismatch(canonical));
    }
    Ok(())
}

pub(in crate::ceph_reconstruction::cephfs) fn validate_range_response(
    descriptor: &CephFsDescriptor,
    locator: &CephFsObjectLocator,
    offset: u64,
    length: usize,
    expected_metadata: Option<&CephFsObjectMetadata>,
    range: &CephFsObjectRange,
) -> Result<(), CephFsObjectReadError> {
    let canonical = locator.canonical();
    let requested_end = u64::try_from(length)
        .ok()
        .and_then(|length| offset.checked_add(length));
    let metadata_matches = expected_metadata.is_none_or(|metadata| {
        metadata.object_size == range.object_size
            && canonical_provenance(&metadata.provenance) == canonical_provenance(&range.provenance)
    });
    if range.filesystem_identity != descriptor.identity
        || range.locator != canonical
        || range.offset != offset
        || range.bytes.len() != length
        || range.object_size == 0
        || requested_end.is_none_or(|end| end > range.object_size)
        || !valid_provenance(&range.provenance)
        || !metadata_matches
    {
        return Err(response_mismatch(canonical));
    }
    Ok(())
}

fn valid_provenance(provenance: &[CephFsObjectReadProvenance]) -> bool {
    if provenance.is_empty() || provenance.len() > MAX_RESPONSE_PROVENANCE {
        return false;
    }
    let mut sources = BTreeSet::new();
    let mut inventories = BTreeSet::new();
    provenance.iter().all(|entry| {
        valid_identity(&entry.data_source_id)
            && valid_identity(&entry.inventory_id)
            && valid_sha256(&entry.object_identity_sha256)
            && sources.insert(entry.data_source_id.as_str())
            && inventories.insert(entry.inventory_id.as_str())
    })
}

fn canonical_provenance(
    provenance: &[CephFsObjectReadProvenance],
) -> BTreeSet<&CephFsObjectReadProvenance> {
    provenance.iter().collect()
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn response_mismatch(locator: String) -> CephFsObjectReadError {
    CephFsObjectReadError::ResponseMismatch { locator }
}
