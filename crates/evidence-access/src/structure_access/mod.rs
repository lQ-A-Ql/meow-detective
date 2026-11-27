mod plan;
mod view;

pub(crate) use plan::{open_discovery_view, plan_discovery_access};
pub use plan::{
    open_verified_view, plan_target_access, verify_derived_view, AccessTarget, TargetAccessError,
    TargetAccessPlan, TargetAccessRequest,
};
pub use view::VerifiedView;
