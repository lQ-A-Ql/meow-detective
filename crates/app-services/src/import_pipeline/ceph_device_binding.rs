use std::path::{Path, PathBuf};

use persistence_sqlite::repositories::ceph_osd_device_binding_repo::{
    CephOsdDeviceBindingAggregate, CephOsdDeviceBindingRecord, CephOsdPvBindingRecord,
};
use persistence_sqlite::repositories::ceph_osd_repo::CephOsdInventoryRecord;
use transport::CommandError;

use crate::datasource_service::UnsupportedImageVolume;

pub(super) fn build_device_binding(
    data_source: &domain::DataSource,
    volume: &UnsupportedImageVolume,
    inventory: &CephOsdInventoryRecord,
) -> Result<CephOsdDeviceBindingAggregate, CommandError> {
    let identity = volume.lvm_identity.as_ref().ok_or_else(|| {
        CommandError::from_service_error("BlueStore logical volume is missing its LVM identity")
    })?;
    validate_identity_shape(identity)?;
    let canonical_source_path = data_source
        .provenance
        .canonical_source_path
        .as_ref()
        .ok_or_else(|| {
            CommandError::from_service_error(
                "BlueStore source has no canonical evidence identity; reattach the source",
            )
        })?;
    let device_size = volume.size_bytes.ok_or_else(|| {
        CommandError::from_service_error("BlueStore logical volume size is unavailable")
    })?;
    if inventory.device_size > device_size {
        return Err(CommandError::from_service_error(
            "BlueStore label size exceeds the persisted LVM logical volume size",
        ));
    }

    let physical_volumes = identity
        .pv_sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            build_pv_binding(
                data_source,
                &inventory.id,
                index,
                source,
                identity.pv_offsets[index],
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let registered_canonical = canonical_source_path.to_path_buf();
    if !physical_volumes.iter().any(|pv| {
        paths_match(
            Path::new(&pv.canonical_source_path),
            registered_canonical.as_path(),
        )
    }) {
        return Err(CommandError::from_service_error(
            "BlueStore LVM identity is not bound to the registered evidence source",
        ));
    }

    Ok(CephOsdDeviceBindingAggregate {
        device: CephOsdDeviceBindingRecord {
            inventory_id: inventory.id.clone(),
            data_source_id: data_source.id.0.clone(),
            source_path: data_source.source_path.display().to_string(),
            canonical_source_path: canonical_source_path.display().to_string(),
            source_kind: source_kind(&data_source.kind)?.to_string(),
            lvm_vg_uuid: identity.vg_uuid.clone(),
            lvm_vg_name: identity.vg_name.clone(),
            lvm_lv_uuid: identity.lv_uuid.clone(),
            lvm_lv_name: identity.lv_name.clone(),
            device_size,
        },
        physical_volumes,
    })
}

fn build_pv_binding(
    data_source: &domain::DataSource,
    inventory_id: &str,
    index: usize,
    source: &crate::datasource_service::LvmPhysicalVolumeSource,
    expected_offset: u64,
) -> Result<CephOsdPvBindingRecord, CommandError> {
    if source.offset != expected_offset {
        return Err(CommandError::from_service_error(
            "BlueStore LVM PV offsets and source identities are inconsistent",
        ));
    }
    if source.pv_uuid.is_empty() {
        return Err(CommandError::from_service_error(
            "BlueStore LVM PV identity is missing its UUID",
        ));
    }
    let kind = source.source_kind.as_ref().unwrap_or(&data_source.kind);
    let path = PathBuf::from(&source.source_path);
    let canonical_path = std::fs::canonicalize(&path).map_err(|error| {
        CommandError::from_service_error(format!(
            "BlueStore LVM PV source cannot be canonicalized: {error}"
        ))
    })?;
    Ok(CephOsdPvBindingRecord {
        inventory_id: inventory_id.to_string(),
        ordinal: u32::try_from(index)
            .map_err(|_| CommandError::from_service_error("too many BlueStore LVM PV sources"))?,
        source_path: path.display().to_string(),
        canonical_source_path: canonical_path.display().to_string(),
        source_kind: source_kind(kind)?.to_string(),
        pv_offset: source.offset,
        pv_uuid: source.pv_uuid.clone(),
        pv_name: source.pv_name.clone(),
    })
}

fn validate_identity_shape(
    identity: &crate::datasource_service::LvmLogicalVolumeIdentity,
) -> Result<(), CommandError> {
    if identity.vg_uuid.is_empty()
        || identity.vg_name.is_empty()
        || identity.lv_uuid.is_empty()
        || identity.lv_name.is_empty()
    {
        return Err(CommandError::from_service_error(
            "BlueStore LVM identity is incomplete",
        ));
    }
    if identity.pv_offsets.is_empty() || identity.pv_offsets.len() != identity.pv_sources.len() {
        return Err(CommandError::from_service_error(
            "BlueStore LVM identity does not contain a complete PV source map",
        ));
    }
    Ok(())
}

fn source_kind(kind: &domain::DataSourceKind) -> Result<&'static str, CommandError> {
    match kind {
        domain::DataSourceKind::E01 => Ok("e01"),
        domain::DataSourceKind::Raw => Ok("raw"),
        domain::DataSourceKind::LogicalDirectory => Err(CommandError::from_service_error(
            "BlueStore LVM devices cannot be bound to logical-directory sources",
        )),
        domain::DataSourceKind::CephRbd => Err(CommandError::unsupported(
            "BlueStore LVM devices cannot be bound to Ceph RBD derived sources",
        )),
    }
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
