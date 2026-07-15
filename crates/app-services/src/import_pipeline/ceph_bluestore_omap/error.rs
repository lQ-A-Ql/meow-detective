use ceph_wire::{BlueStoreOmapKeyFamily, CephWireError};
use thiserror::Error;

use super::types::BlueStoreOmapScope;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BlueStoreOmapError {
    #[error("BlueStore OMAP key decode failed: {0}")]
    KeyDecode(#[from] CephWireError),

    #[error("BlueStore OMAP value decode failed for {field}: {source}")]
    ValueDecode {
        field: &'static str,
        #[source]
        source: CephWireError,
    },

    #[error("BlueStore OMAP value for {field} has {remaining} trailing bytes")]
    TrailingValue {
        field: &'static str,
        remaining: usize,
    },

    #[error("BlueStore OMAP {field} is invalid: {reason}")]
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },

    #[error("BlueStore OMAP scope {scope:?} is invalid: {reason}")]
    InvalidScope {
        scope: BlueStoreOmapScope,
        reason: &'static str,
    },

    #[error("BlueStore OMAP scope {scope:?} has no header")]
    MissingHeader { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP scope {scope:?} has no tail")]
    UnclosedScope { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP scope {scope:?} has a duplicate header")]
    DuplicateHeader { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP scope {scope:?} has a duplicate field {field}")]
    DuplicateField {
        scope: BlueStoreOmapScope,
        field: &'static str,
    },

    #[error("BlueStore OMAP scope {scope:?} has a duplicate directory mapping")]
    DuplicateDirectoryMapping { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP scope {scope:?} has conflicting directory mappings")]
    ConflictingDirectoryMapping { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP owner conflict for nid {nid} and family {family:?}")]
    OwnerConflict {
        nid: u64,
        family: BlueStoreOmapKeyFamily,
    },

    #[error("BlueStore OMAP owner name for {kind} is invalid: {reason}")]
    InvalidOwnerName {
        kind: &'static str,
        reason: &'static str,
    },

    #[error("BlueStore OMAP scope {scope:?} is duplicated while merging fragments")]
    DuplicateScope { scope: BlueStoreOmapScope },

    #[error("BlueStore OMAP fragment cannot merge while a scope is open")]
    MergeWithOpenScope,

    #[error("BlueStore OMAP fragment limit for {resource} is {limit}")]
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },

    #[error("BlueStore OMAP header for image {image_id} is duplicated")]
    DuplicateRbdHeader { image_id: String },
}

pub(super) fn invalid_field(field: &'static str, reason: &'static str) -> BlueStoreOmapError {
    BlueStoreOmapError::InvalidField { field, reason }
}
