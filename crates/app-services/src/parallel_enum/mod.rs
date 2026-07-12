//! Parallel filesystem enumeration.
//!
//! Partition coordination, evidence reads, and staging writes remain separate
//! so evidence-image I/O stays bounded and each source database keeps one
//! writer per partition staging database.

mod batch_sink;
mod coordinator;
pub mod error;
mod ntfs;
mod partition_work;
mod progress;

pub use coordinator::{default_worker_count, enumerate_partitions_parallel, resolve_worker_count};
pub use error::ParallelEnumError;
pub use partition_work::{PartitionResult, PartitionWork};

#[cfg(test)]
#[path = "../../tests/unit/parallel_enum/mod.rs"]
mod tests;
