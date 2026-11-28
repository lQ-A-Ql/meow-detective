mod activity;
mod activity_registration;
mod analysis;
pub mod analysis_inputs;
mod coordinator;
mod dag;
mod execution;
pub mod frontier;
mod proof;
mod proof_state;
mod readiness;
mod registered;
mod registry;
mod report;

pub use activity::{source_scope_id, ActivityDefinition, RuntimeError};
pub use analysis::{
    analysis_source_seeds, register_layer_analysis, LayerAnalysisProfile, LayerAnalysisResult,
};
pub use analysis_inputs::{
    match_inputs, AnalysisInputError, AnalysisInputMatch, AnalysisInputRequirement,
    MinimumIntegrity,
};
pub use coordinator::{run, RuntimeReport, RuntimeStart};
pub use dag::ActivityDag;
pub use execution::{ActivityOutput, ExecutionContext};
pub use proof::ActivityProof;
pub use readiness::Readiness;
pub use registered::{registered_registry, registered_source_seed};
pub use registry::{
    BindingContext, BindingDecision, ExecutableDescriptor, ExecutableRegistry, StructureOperation,
};
