//! Source runtime factories and identity helpers.
//!
//! Publication leases and resource accounting live in
//! `source-kernel::source_runtime`; this module owns the reader factory and the
//! per-source runtime identities built on top of it.

pub mod factory;
pub mod identity;
pub mod partition;
pub mod snapshot;

pub use factory::{open_factory, open_factory_for_case, open_factory_with_cancel};
pub use identity::{SourceIdentity, SourceRuntime};
pub use partition::PartitionReadPlan;
pub use source_kernel::source_runtime::lease::{
    invalidate_case, invalidate_source, SourceInvalidated, SourcePublicationLease,
};
pub use source_kernel::source_runtime::{lease, resources};
