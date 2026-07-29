use std::{path::Path, time::Duration};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{
    read_volume_identities, unlock_volume_with_password as unlock_password,
    unlock_volume_with_recovery_password as unlock_recovery, MetadataFingerprint, Passphrase,
    VerifiedUnlock,
};

use crate::bitlocker_runtime::BitLockerRuntimeError;
use persistence_sqlite::repositories::bitlocker_restore_intent_repo::BitLockerRestoreIntentRepo;

use super::{
    activation::activate_verified,
    audit::{self, BitLockerAudit},
    source::{
        open_partition_window, open_registered_plaintext, open_source_read_only,
        probe_plaintext_filesystem,
    },
    status::{build_status, matching_identity},
    BitLockerRuntimeContext, BitLockerServiceError,
};

const LOCK_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

pub fn inspect_bitlocker_volume(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let _read_lease = runtimes
        .preview_runtime
        .begin_session(case_id, data_source_id)?;
    let source = open_source_read_only(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
    )?;
    let mut window = open_partition_window(&source)?;
    let identities = read_volume_identities(&mut window)?;
    let registered = match runtimes.bitlocker_runtime.resolve_for_identities(
        &case_id.0,
        &data_source_id.0,
        partition_index as usize,
        &identities,
    ) {
        Ok(value) => Some(value),
        Err(BitLockerRuntimeError::Locked) => None,
        Err(error) => return Err(error.into()),
    };
    let (identity, stored_key_available) = status_identity(
        &identities,
        registered
            .as_ref()
            .map(|value| value.scope().metadata_fingerprint()),
        runtimes.key_store,
    )?;
    let plaintext_filesystem = if registered.is_some() {
        let mut plaintext =
            open_registered_plaintext(&source, case_id, runtimes.bitlocker_runtime)?;
        probe_plaintext_filesystem(plaintext.as_mut())?
    } else {
        None
    };
    Ok(build_status(
        &data_source_id.0,
        partition_index,
        identity,
        identities.len(),
        registered.is_some(),
        stored_key_available,
        plaintext_filesystem,
    ))
}

pub fn unlock_bitlocker_with_password(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    credential: Passphrase,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    unlock_with(
        UnlockContext {
            case_conn,
            case_root,
            case_id,
            data_source_id,
            partition_index,
            runtimes,
        },
        credential,
        UnlockMethod::Password,
    )
}

pub fn unlock_bitlocker_with_recovery_password(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    credential: Passphrase,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    unlock_with(
        UnlockContext {
            case_conn,
            case_root,
            case_id,
            data_source_id,
            partition_index,
            runtimes,
        },
        credential,
        UnlockMethod::RecoveryPassword,
    )
}

struct UnlockContext<'a> {
    case_conn: &'a Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    data_source_id: &'a DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'a>,
}

#[derive(Clone, Copy)]
enum UnlockMethod {
    Password,
    RecoveryPassword,
}

impl UnlockMethod {
    fn apply(
        self,
        window: &mut evidence_core::PartitionWindowReader,
        credential: &Passphrase,
    ) -> volume_bitlocker::Result<VerifiedUnlock> {
        match self {
            Self::Password => unlock_password(window, credential),
            Self::RecoveryPassword => unlock_recovery(window, credential),
        }
    }

    fn audit_name(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::RecoveryPassword => "recoveryPassword",
        }
    }
}

fn unlock_with(
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
    let persisted_blob = verified.persisted_key_blob();
    let activated = match activate_verified(
        &source,
        context.case_id,
        context.partition_index,
        context.runtimes.preview_runtime,
        context.runtimes.bitlocker_runtime,
        verified,
    ) {
        Ok(value) => value,
        Err(error) => {
            audit_unlock(
                &context,
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
            &context,
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
            &context,
            activated.fingerprint.as_str(),
            method,
            "failed",
            Some("BITLOCKER_RESTORE_INTENT_STORE_FAILED"),
        );
        return Err(error.into());
    }
    audit_unlock(
        &context,
        activated.fingerprint.as_str(),
        method,
        "success",
        None,
    );
    Ok(build_status(
        &context.data_source_id.0,
        context.partition_index,
        &activated.identity,
        identities.len(),
        true,
        true,
        activated.plaintext_filesystem,
    ))
}

fn status_identity<'a>(
    identities: &'a [volume_bitlocker::VolumeIdentity],
    registered: Option<&MetadataFingerprint>,
    key_store: &dyn super::BitLockerKeyStore,
) -> Result<(&'a volume_bitlocker::VolumeIdentity, bool), BitLockerServiceError> {
    if let Some(fingerprint) = registered {
        let identity = matching_identity(identities, fingerprint);
        return Ok((identity, key_store.contains(fingerprint)?));
    }
    for identity in identities {
        let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
        if key_store.contains(&fingerprint)? {
            return Ok((identity, true));
        }
    }
    Ok((&identities[0], false))
}

fn audit_unlock(
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
        },
    );
}

pub fn lock_bitlocker_volume(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let before = inspect_bitlocker_volume(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    )?;
    let drained = runtimes.preview_runtime.retire_source_and_drain(
        &case_id.0,
        &data_source_id.0,
        LOCK_DRAIN_TIMEOUT,
    )?;
    if !drained {
        let _ = runtimes
            .preview_runtime
            .reactivate_source(&case_id.0, &data_source_id.0);
        audit_lock(
            case_conn,
            case_id,
            data_source_id,
            partition_index,
            &before,
            "timeout",
        );
        return Err(BitLockerServiceError::DrainTimeout);
    }
    let invalidated = runtimes.bitlocker_runtime.invalidate_partition(
        &case_id.0,
        &data_source_id.0,
        partition_index as usize,
    );
    let reactivated = runtimes
        .preview_runtime
        .reactivate_source(&case_id.0, &data_source_id.0);
    invalidated?;
    reactivated?;
    audit_lock(
        case_conn,
        case_id,
        data_source_id,
        partition_index,
        &before,
        "success",
    );
    Ok(BitLockerVolumeStatusDto {
        unlocked: false,
        plaintext_filesystem: None,
        ..before
    })
}

fn audit_lock(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    status: &BitLockerVolumeStatusDto,
    outcome: &str,
) {
    audit::record(
        case_conn,
        BitLockerAudit {
            case_id: &case_id.0,
            data_source_id: &data_source_id.0,
            partition_index,
            metadata_fingerprint: Some(&status.metadata_fingerprint),
            operation: "lock",
            outcome,
            error_code: (outcome == "timeout").then_some("BITLOCKER_LOCK_TIMEOUT"),
        },
    );
}
