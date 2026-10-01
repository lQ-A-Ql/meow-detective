use super::super::audit::{self, BitLockerAudit};
use super::{UnlockContext, UnlockMethod};
pub(in crate::bitlocker_service) fn audit_unlock(
    context: &UnlockContext<'_>,
    metadata_fingerprint: &str,
    method: UnlockMethod,
    outcome: &str,
    error_code: Option<&str>,
) {
    audit::record(
        context.case_conn,
        BitLockerAudit {
            case_id: &context.case_id.0,
            data_source_id: &context.data_source_id.0,
            partition_index: context.partition_index,
            metadata_fingerprint: Some(metadata_fingerprint),
            operation: method.audit_name(),
            outcome,
            error_code,
            extra_details: None,
        },
    );
}
