use super::super::{
    context::BitLockerRuntimeContext,
    source::{
        open_partition_window, open_registered_plaintext, open_source_read_only,
        probe_plaintext_filesystem,
    },
    status::build_status,
    BitLockerServiceError,
};

use super::super::status::matching_identity;
use crate::bitlocker_runtime::BitLockerRuntimeError;
use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::{read_volume_identities, MetadataFingerprint};

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

fn status_identity<'a>(
    identities: &'a [volume_bitlocker::VolumeIdentity],
    registered: Option<&MetadataFingerprint>,
    key_store: &dyn super::super::BitLockerKeyStore,
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
