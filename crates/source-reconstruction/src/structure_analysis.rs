mod adapter;
mod execution;
mod execution_permit;
mod graph_builder;
mod planner;
mod query;
mod request;
mod scheduler;

pub use adapter::{
    AdapterRegistry, BTRFS_ADAPTER_ID, CEPHFS_ADAPTER_ID, COLLECTION_ADAPTER_ID,
    COLLECTION_ADAPTER_VERSION, FILESYSTEM_ADAPTER_ID, FILESYSTEM_ADAPTER_VERSION, LVM_ADAPTER_ID,
    MAX_ADAPTERS, MDRAID_ADAPTER_ID, MDRAID_ADAPTER_VERSION, RBD_ADAPTER_ID,
    REGISTERED_SOURCE_ADAPTER_ID,
};
pub use evidence_access::structure_access::{
    open_verified_view, plan_registered_access, plan_target_access, verify_registered_access,
    AccessTarget, RegisteredAccessRequest, TargetAccessError, TargetAccessPlan,
    TargetAccessRequest, VerifiedView,
};
pub(crate) use execution::execute_structure_analysis;
pub use execution::run_structure_analysis;
pub use planner::plan_structure_analysis;
pub use query::get_structure_graph;
pub use request::{
    StructureAnalysisError, StructureAnalysisRequest, StructureAnalysisRun, StructureGraphRequest,
    StructureWorkResult, StructureWorkStatus,
};
pub use scheduler::{schedule_work_items, MAX_WORK_ITEMS};

#[cfg(test)]
#[path = "../tests/unit/structure_analysis.rs"]
mod tests;
