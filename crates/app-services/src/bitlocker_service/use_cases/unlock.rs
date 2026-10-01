use super::super::{
    source::{open_partition_window, open_source_read_only},
    BitLockerServiceError,
};
use super::unlock_audit::audit_unlock;
use super::{complete_verified_unlock, UnlockContext, UnlockMethod};
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{read_volume_identities, MetadataFingerprint, Passphrase};
pub(in crate::bitlocker_service) fn unlock_with(
    context: UnlockContext<'_>,
    credential: Passphrase,
    method: UnlockMethod,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let _read_lease = context
        .runtimes
        .preview_runtime
        .begin_session(context.case_id, context.data_source_id)?;
    let source = open_source_read_only(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
        context.partition_index,
    )?;
    let mut window = open_partition_window(&source)?;
    let identities = read_volume_identities(&mut window)?;
    let fingerprint = MetadataFingerprint::from_metadata(&identities[0].metadata);
    let verified = match method.apply(&mut window, &credential) {
        Ok(value) => value,
        Err(error) => {
            audit_unlock(
                &context,
                fingerprint.as_str(),
                method,
                "failed",
                Some(error.code()),
            );
            return Err(error.into());
        }
    };
    complete_verified_unlock(&context, &source, &identities, verified, method, None)
}
