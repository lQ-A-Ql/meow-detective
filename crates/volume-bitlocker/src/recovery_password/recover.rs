use crate::{FveMetadata, MetadataFingerprint, RecoveredVmk};

use super::error::RecoveryPasswordRecoveryError;
use super::formatter::format_material;
use super::material::RecoveryPasswordMaterial;
use super::protector::{select_protector, RecoveryPasswordProtectorIdentity};
use super::provenance::{RecoveredRecoveryPassword, RecoveryPasswordProvenance};
use super::reverse_datum::ReverseRecoveryDatum;

/// Authenticates and reconstructs one exact numerical recovery password.
pub fn recover_recovery_password(
    metadata: &FveMetadata,
    protector_identity: RecoveryPasswordProtectorIdentity,
    vmk: &RecoveredVmk,
) -> Result<RecoveredRecoveryPassword, RecoveryPasswordRecoveryError> {
    let protector = select_protector(metadata, protector_identity)?;
    let reverse = ReverseRecoveryDatum::from_protector(protector)?;
    let plaintext = reverse.authenticate(vmk)?;
    let material = RecoveryPasswordMaterial::parse(&plaintext)?;
    let password = format_material(&material);
    let metadata_fingerprint = MetadataFingerprint::from_metadata(metadata);
    let provenance = RecoveryPasswordProvenance::new(
        metadata.volume_guid,
        protector_identity,
        &metadata_fingerprint,
        reverse.encoded(),
    );
    Ok(RecoveredRecoveryPassword::new(password, provenance))
}
