mod latest_state;
mod model;

pub use latest_state::{reduce_latest_state, reduce_latest_state_ref};
pub use model::{
    KeyVersion, KeyVersionKind, LatestState, LatestStateError, LatestStateLimits, LatestStateRef,
    MergeOperator,
};
