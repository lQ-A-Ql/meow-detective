mod diagnostics;
mod discovery;
mod expansion;
mod model;
pub(super) mod source_identity;

pub use expansion::{expand_lvm_pool_candidates, expand_lvm_pool_candidates_with_sources};
pub(crate) use source_identity::{lvm_source_fingerprint, normalize_lvm_uuid_for_match};
