use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{
    read_volume_identities, restore_volume_from_persisted_key, MetadataFingerprint, VerifiedUnlock,
    VolumeIdentity,
};

use super::{
    activation::activate_verified,
    audit::{self, BitLockerAudit},
    inspect_bitlocker_volume,
    source::{open_partition_window, open_source_read_only},
    status::build_status,
    BitLockerRuntimeContext, BitLockerServiceError,
};

pub fn restore_persisted_bitlocker_key(
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
    let (identity, fingerprint, verified) = load_verified_unlock(&identities, runtimes.key_store)
        .inspect_err(|error| {
        audit_persistence(
            case_conn,
            case_id,
            data_source_id,
            partition_index,
            identities.first(),
            "restoreKey",
            "failed",
            error_code(error),
        );
    })?;
    let activated = activate_verified(
        &source,
        case_id,
        partition_index,
        runtimes.preview_runtime,
        runtimes.bitlocker_runtime,
        verified,
    )
    .inspect_err(|_error| {
        audit_persistence(
            case_conn,
            case_id,
            data_source_id,
            partition_index,
            Some(&identity),
            "restoreKey",
            "failed",
            Some("BITLOCKER_PLAINTEXT_PROBE_FAILED"),
        );
    })?;
    audit_persistence(
        case_conn,
        case_id,
        data_source_id,
        partition_index,
        Some(&activated.identity),
        "restoreKey",
        "success",
        None,
    );
    debug_assert_eq!(fingerprint, activated.fingerprint);
    Ok(build_status(
        &data_source_id.0,
        partition_index,
        &activated.identity,
        identities.len(),
        true,
        true,
        activated.plaintext_filesystem,
    ))
}

pub fn forget_persisted_bitlocker_key(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let identity = {
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
        for candidate in &identities {
            let fingerprint = MetadataFingerprint::from_metadata(&candidate.metadata);
            runtimes.key_store.delete(&fingerprint)?;
        }
        identities[0].clone()
    };
    audit_persistence(
        case_conn,
        case_id,
        data_source_id,
        partition_index,
        Some(&identity),
        "forgetKey",
        "success",
        None,
    );
    inspect_bitlocker_volume(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    )
}

fn load_verified_unlock(
    identities: &[VolumeIdentity],
    key_store: &dyn super::BitLockerKeyStore,
) -> Result<(VolumeIdentity, MetadataFingerprint, VerifiedUnlock), BitLockerServiceError> {
    for identity in identities {
        let fingerprint = MetadataFingerprint::from_metadata(&identity.metadata);
        if let Some(blob) = key_store.load(&fingerprint)? {
            let verified = restore_volume_from_persisted_key(identity.clone(), blob)?;
            return Ok((identity.clone(), fingerprint, verified));
        }
    }
    Err(BitLockerServiceError::StoredKeyNotFound)
}

#[allow(clippy::too_many_arguments)]
fn audit_persistence(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    identity: Option<&VolumeIdentity>,
    operation: &str,
    outcome: &str,
    error_code: Option<&str>,
) {
    let fingerprint = identity.map(|value| MetadataFingerprint::from_metadata(&value.metadata));
    audit::record(
        case_conn,
        BitLockerAudit {
            case_id: &case_id.0,
            data_source_id: &data_source_id.0,
            partition_index,
            metadata_fingerprint: fingerprint.as_ref().map(MetadataFingerprint::as_str),
            operation,
            outcome,
            error_code,
        },
    );
}

fn error_code(error: &BitLockerServiceError) -> Option<&'static str> {
    use transport::ServiceErrorCategory;
    error.code()
}
