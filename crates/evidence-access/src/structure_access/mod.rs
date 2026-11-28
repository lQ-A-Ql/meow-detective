mod deep;
mod plan;
mod registered;
pub use registered::{
    plan_registered_access, verify_registered_access, RegisteredAccessRequest,
    CEPHFS_FILE_TARGET_PREFIX, REGISTERED_READ_POLICY,
};

pub(crate) use plan::{open_discovery_view, plan_discovery_access};
pub use plan::{
    open_verified_view, plan_target_access, verify_derived_view, AccessTarget, TargetAccessError,
    TargetAccessPlan, TargetAccessRequest,
};
pub use source_kernel::source_runtime::range_view::VerifiedView;
