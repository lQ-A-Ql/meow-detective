use transport::dto::{BitLockerProtectorDto, BitLockerVolumeStatusDto};
use volume_bitlocker::{MetadataFingerprint, ProtectorKind, VolumeIdentity};

pub(crate) fn build_status(
    data_source_id: &str,
    partition_index: u32,
    identity: &VolumeIdentity,
    metadata_copy_count: usize,
    unlocked: bool,
    stored_key_available: bool,
    plaintext_filesystem: Option<String>,
) -> BitLockerVolumeStatusDto {
    let metadata = &identity.metadata;
    let inventory = metadata.protector_inventory();
    let protectors = metadata
        .protector_codes()
        .into_iter()
        .zip(inventory.protectors().iter().copied())
        .map(|(code, kind)| protector_dto(code, kind))
        .collect::<Vec<_>>();
    BitLockerVolumeStatusDto {
        data_source_id: data_source_id.to_string(),
        partition_index,
        unlocked,
        encryption_method: metadata.encryption_method.label().to_string(),
        encryption_method_code: metadata.encryption_method_code,
        decryptable: metadata.encryption_method.is_decryptable(),
        bytes_per_sector: identity.bytes_per_sector,
        metadata_fingerprint: MetadataFingerprint::from_metadata(metadata)
            .as_str()
            .to_string(),
        metadata_copy_count: u32::try_from(metadata_copy_count).unwrap_or(u32::MAX),
        supports_password: inventory.protectors().contains(&ProtectorKind::Password),
        supports_recovery_password: inventory
            .protectors()
            .contains(&ProtectorKind::RecoveryPassword),
        stored_key_available,
        protectors,
        plaintext_filesystem,
        recovery_password_reconstruction: None,
    }
}

pub(crate) fn matching_identity<'a>(
    identities: &'a [VolumeIdentity],
    fingerprint: &MetadataFingerprint,
) -> &'a VolumeIdentity {
    identities
        .iter()
        .find(|identity| MetadataFingerprint::from_metadata(&identity.metadata) == *fingerprint)
        .unwrap_or(&identities[0])
}

fn protector_dto(code: u16, kind: ProtectorKind) -> BitLockerProtectorDto {
    BitLockerProtectorDto {
        code,
        kind: protector_kind_name(kind).to_string(),
        label: kind.label().to_string(),
        unlockable: kind.is_unlockable(),
    }
}

fn protector_kind_name(kind: ProtectorKind) -> &'static str {
    match kind {
        ProtectorKind::ClearKey => "clearKey",
        ProtectorKind::RecoveryPassword => "recoveryPassword",
        ProtectorKind::Password => "password",
        ProtectorKind::Tpm => "tpm",
        ProtectorKind::StartupKey => "startupKey",
        ProtectorKind::Unknown(_) => "unknown",
    }
}
