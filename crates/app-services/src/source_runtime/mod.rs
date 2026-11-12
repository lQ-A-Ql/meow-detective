pub(crate) mod factory;
mod identity;
mod partition;
pub(crate) mod resume;
mod snapshot;

pub use factory::{open_factory, open_factory_with_cancel};
pub use identity::{SourceIdentity, SourceRuntime};
pub use partition::PartitionReadPlan;
pub use source_kernel::source_runtime::lease::{
    invalidate_case, invalidate_source, SourceInvalidated, SourcePublicationLease,
};
pub use source_kernel::source_runtime::{lease, resources};

#[cfg(test)]
#[path = "../../tests/unit/source_runtime_lease.rs"]
mod tests;
