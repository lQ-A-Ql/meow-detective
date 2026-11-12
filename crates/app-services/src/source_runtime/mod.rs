//! Source runtime facade.
//!
//! The reader factory and per-source runtime identities live in
//! `evidence-access::source_runtime`; publication leases and resource
//! accounting still come straight from `source-kernel`; import resume
//! identity lives in `import-core`.

pub use ::evidence_access::source_runtime::{factory, identity, partition, snapshot};
pub use ::evidence_access::source_runtime::{
    open_factory, open_factory_for_case, open_factory_with_cancel, PartitionReadPlan,
    SourceIdentity, SourceRuntime,
};
pub use source_kernel::source_runtime::lease::{
    invalidate_case, invalidate_source, SourceInvalidated, SourcePublicationLease,
};
pub use source_kernel::source_runtime::{lease, resources};

#[cfg(test)]
#[path = "../../tests/unit/source_runtime_lease.rs"]
mod tests;
