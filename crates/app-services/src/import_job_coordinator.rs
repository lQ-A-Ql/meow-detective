use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub use crate::import_group_gate::{ImportGroupGateError, ImportGroupLease};

const IMPORT_GROUP_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

pub fn acquire_import_group_slot(
    case_id: &str,
    cancel_token: &AtomicBool,
) -> Result<ImportGroupLease, ImportGroupGateError> {
    crate::import_group_gate::global_import_group_gate().acquire(
        case_id,
        cancel_token,
        IMPORT_GROUP_WAIT_TIMEOUT,
    )
}
