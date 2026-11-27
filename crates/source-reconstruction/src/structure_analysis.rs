mod adapter;
mod execution;
mod graph_builder;
mod planner;
mod query;
mod request;
mod scheduler;

pub use adapter::{
    AdapterRegistry, COLLECTION_ADAPTER_ID, COLLECTION_ADAPTER_VERSION, FILESYSTEM_ADAPTER_ID,
    FILESYSTEM_ADAPTER_VERSION, MAX_ADAPTERS, MDRAID_ADAPTER_ID, MDRAID_ADAPTER_VERSION,
};
pub use evidence_access::structure_access::{
    open_verified_view, plan_registered_mdraid_access, plan_target_access, verify_derived_view,
    AccessTarget, RegisteredMdRaidAccessRequest, TargetAccessError, TargetAccessPlan,
    TargetAccessRequest, VerifiedView,
};
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
