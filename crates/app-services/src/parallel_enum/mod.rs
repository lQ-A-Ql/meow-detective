//! Parallel filesystem enumeration.
//!
//! Partition coordination, evidence reads, and staging writes remain separate
//! so evidence-image I/O stays bounded and each source database keeps one
//! writer per partition staging database.

mod batch_sink;
mod coordinator;
pub mod error;
mod ntfs;
mod ntfs_entry;
mod ntfs_plan;
mod ntfs_sink;
mod partition_enumeration;
mod partition_work;
mod progress;

pub(crate) use coordinator::enumerate_partitions_profiled;
pub use coordinator::{
    default_worker_count, effective_worker_count, enumerate_partitions_parallel,
    resolve_worker_count,
};
pub use error::ParallelEnumError;
pub use ntfs_plan::{NtfsPartitionPlan, PartitionEnumStrategy};
pub use partition_work::{PartitionResult, PartitionWork};

pub(crate) use ntfs_entry::enumerate_ntfs_reader_to_staging;

#[cfg(test)]
#[path = "../../tests/unit/parallel_enum/mod.rs"]
mod tests;
