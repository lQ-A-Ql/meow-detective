//! Verify source integrity and build the read-only guest parent view.
use super::{MountServiceError, PreparedPhysicalImageKind};
use domain::{CaseId, DataSourceId};
use evidence_block::{open_block_provider, BlockProvider, EvidenceImageKind};
use evidence_emulation::{NtfsBootDisk, ParentIdentity, SourceVolumeMapping};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use transport::dto::case::DerivedNtfsDiskDto;

pub struct PreparedEmulationParent {
    pub provider: Arc<dyn BlockProvider>,
    pub identity: ParentIdentity,
    pub source_volume: Option<SourceVolumeMapping>,
    pub derived_ntfs: Option<DerivedNtfsDiskDto>,
}

pub fn prepare_emulation_parent(
    conn: &Connection,
    case_root: &std::path::Path,
    case: &CaseId,
    source: &DataSourceId,
    recovery_media_available: bool,
) -> Result<PreparedEmulationParent, MountServiceError> {
    let geometry =
        super::prepare_emulation_disk_layout(conn, case, source, recovery_media_available)?;
    let prepared = super::prepare_emulation_source(conn, case_root, case, source)?;
    let kind = match prepared.image_kind {
        PreparedPhysicalImageKind::E01 => EvidenceImageKind::E01,
        PreparedPhysicalImageKind::Raw => EvidenceImageKind::Raw,
    };
    let backing = open_block_provider(&prepared.source_path, kind)?;
    let mut parent = PreparedEmulationParent {
        identity: ParentIdentity::new(backing.len(), prepared.parent_sha256)?,
        provider: backing,
        source_volume: None,
        derived_ntfs: None,
    };
    if let Some(volume) = geometry {
        let length = volume
            .verified_extent_bytes
            .ok_or(MountServiceError::DirectVolumeRecoveryRequired)?;
        let signature =
            u32::from_le_bytes(prepared.parent_sha256[..4].try_into().unwrap_or([1; 4])).max(1);
        let selected = NtfsBootDisk::new(parent.provider.clone(), length, signature)?;
        let mapping = selected.mapping();
        parent.derived_ntfs = Some(DerivedNtfsDiskDto {
            layout_version: 1,
            source_media_bytes: mapping.source_media_length,
            source_volume_offset: 0,
            source_volume_bytes: mapping.source_volume_length,
            source_volume_serial: volume.volume_serial,
            guest_partition_index: 1,
            guest_partition_offset: mapping.guest_volume_offset,
            guest_disk_bytes: mapping.guest_length()?,
            generated_disk_signature: signature,
            source_hidden_sectors: mapping.original_hidden_sectors,
            guest_hidden_sectors: (mapping.guest_volume_offset / 512) as u32,
            generated_mbr_sha256: hex::encode(Sha256::digest(selected.mbr())),
            boot_repair_required: true,
        });
        parent.source_volume = Some(mapping);
        parent.identity = ParentIdentity::new(selected.len(), prepared.parent_sha256)?;
        parent.provider = Arc::new(selected);
    }
    Ok(parent)
}
