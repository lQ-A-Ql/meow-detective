use super::super::{
    activation::activate_verified, source::BitLockerSource, status::build_status,
    BitLockerServiceError,
};
use super::unlock_audit::audit_unlock;
use super::{UnlockContext, UnlockMethod};
use persistence_sqlite::repositories::bitlocker_restore_intent_repo::BitLockerRestoreIntentRepo;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{MetadataFingerprint, VerifiedUnlock};
#[allow(clippy::too_many_arguments)]
pub(in crate::bitlocker_service) fn complete_verified_unlock(
    context: &UnlockContext<'_>,
    source: &BitLockerSource,
    identities: &[volume_bitlocker::VolumeIdentity],
    verified: VerifiedUnlock,
    method: UnlockMethod,
    recovery_reveal: Option<transport::dto::RecoveryPasswordReconstructionDto>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let fingerprint = MetadataFingerprint::from_metadata(&verified.identity().metadata);
    let persisted_blob = verified.persisted_key_blob();
    let activated = match activate_verified(
        source,
        context.case_id,
        context.partition_index,
        context.runtimes.preview_runtime,
        context.runtimes.bitlocker_runtime,
        verified,
    ) {
        Ok(value) => value,
        Err(error) => {
            audit_unlock(
                context,
                fingerprint.as_str(),
                method,
                "failed",
                Some("BITLOCKER_PLAINTEXT_PROBE_FAILED"),
            );
            return Err(error);
        }
    };
    if let Err(error) = context
        .runtimes
        .key_store
        .store(&activated.fingerprint, persisted_blob)
    {
        context.runtimes.bitlocker_runtime.invalidate_partition(
            &context.case_id.0,
            &context.data_source_id.0,
            context.partition_index as usize,
        )?;
        audit_unlock(
            context,
            activated.fingerprint.as_str(),
            method,
            "failed",
            Some("BITLOCKER_KEY_STORE_FAILED"),
        );
        return Err(error.into());
    }
    if let Err(error) = BitLockerRestoreIntentRepo::new(context.case_conn).upsert_enabled(
        context.data_source_id,
        context.partition_index,
        activated.fingerprint.as_str(),
    ) {
        let _ = context.runtimes.key_store.delete(&activated.fingerprint);
        context.runtimes.bitlocker_runtime.invalidate_partition(
            &context.case_id.0,
            &context.data_source_id.0,
            context.partition_index as usize,
        )?;
        audit_unlock(
            context,
            activated.fingerprint.as_str(),
            method,
            "failed",
            Some("BITLOCKER_RESTORE_INTENT_STORE_FAILED"),
        );
        return Err(error.into());
    }
    audit_unlock(
        context,
        activated.fingerprint.as_str(),
        method,
        "success",
        None,
    );
    let mut status = build_status(
        &context.data_source_id.0,
        context.partition_index,
        &activated.identity,
        identities.len(),
        true,
        true,
        activated.plaintext_filesystem,
    );
    status.recovery_password_reconstruction = recovery_reveal;
    Ok(status)
}
