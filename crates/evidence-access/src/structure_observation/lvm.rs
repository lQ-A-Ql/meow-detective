use domain::{
    structure::{
        AcquisitionId, NativeObjectId, NativeObservation, NativeVersion, ObservationIndex,
        RevisionConsistency, SourceRevisionRef, StorageObjectVersion, StorageObjectVersionKind,
    },
    CaseId, StorageObjectKind,
};
use persistence_sqlite::repositories::storage_object_repo::{
    StorageObjectRecord, StorageObjectRepo,
};
use sha2::{Digest, Sha256};

use super::StructureObservationError;

pub(super) fn bind_observation_objects(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    acquisition_id: &str,
    source: &SourceRevisionRef,
    index: &ObservationIndex,
    observation: &NativeObservation,
) -> Result<(), StructureObservationError> {
    match &observation.version {
        NativeVersion::LvmLabel => {
            let Some(pv_uuid) = observation.native_identity.strip_prefix("lvm-pv:") else {
                return Ok(());
            };
            bind_version(
                conn,
                case_id,
                acquisition_id,
                source,
                index,
                pv_uuid,
                "lvm.pv_uuid",
                StorageObjectKind::LvmPhysicalVolume,
                format!("PV {pv_uuid}"),
                NativeVersion::LvmLabel,
                &observation.details,
            )
        }
        NativeVersion::LvmCommitted { vg_uuid, seqno } => {
            let mapping = normalized_vg_mapping(&observation.details);
            bind_version(
                conn,
                case_id,
                acquisition_id,
                source,
                index,
                vg_uuid,
                "lvm.vg_uuid",
                StorageObjectKind::VolumeGroup,
                format!("VG {vg_uuid}"),
                NativeVersion::LvmCommitted {
                    vg_uuid: vg_uuid.clone(),
                    seqno: *seqno,
                },
                &mapping,
            )?;
            bind_logical_volumes(
                conn,
                case_id,
                acquisition_id,
                source,
                index,
                vg_uuid,
                *seqno,
                &observation.details,
            )
        }
        NativeVersion::Md { .. }
        | NativeVersion::CephRbd { .. }
        | NativeVersion::CephFs { .. }
        | NativeVersion::Btrfs { .. } => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_logical_volumes(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    acquisition_id: &str,
    source: &SourceRevisionRef,
    index: &ObservationIndex,
    vg_uuid: &str,
    seqno: u64,
    details: &serde_json::Value,
) -> Result<(), StructureObservationError> {
    let Some(volumes) = details
        .get("logicalVolumes")
        .and_then(|value| value.as_array())
    else {
        return Ok(());
    };
    for volume in volumes {
        let Some(lv_uuid) = volume.get("uuid").and_then(|value| value.as_str()) else {
            continue;
        };
        let name = volume
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(lv_uuid);
        bind_version(
            conn,
            case_id,
            acquisition_id,
            source,
            index,
            lv_uuid,
            "lvm.lv_uuid",
            StorageObjectKind::LogicalVolume,
            format!("LV {name}"),
            NativeVersion::LvmCommitted {
                vg_uuid: vg_uuid.to_owned(),
                seqno,
            },
            volume,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn bind_version(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    acquisition_id: &str,
    source: &SourceRevisionRef,
    index: &ObservationIndex,
    native_id: &str,
    identity_space: &str,
    object_kind: StorageObjectKind,
    name: String,
    native_version: NativeVersion,
    mapping_value: &serde_json::Value,
) -> Result<(), StructureObservationError> {
    let native_object_id = NativeObjectId::new(
        "fs-lvm",
        identity_space,
        native_id,
        format!("{}:{}", case_id.0, acquisition_id),
    )
    .map_err(|error| {
        StructureObservationError::Database(persistence_sqlite::DbError::System(error.to_string()))
    })?;
    let object_id = native_object_id.storage_object_id();
    let repository = StorageObjectRepo::new(conn);
    repository.insert_if_absent(&StorageObjectRecord {
        id: object_id,
        case_id: case_id.0.clone(),
        object_kind: object_kind.as_str().into(),
        name,
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: serde_json::json!({
            "acquisitionId": acquisition_id,
            "nativeIdentity": index.native_identity,
            "observationId": index.id.0,
        })
        .to_string(),
    })?;
    let version = StorageObjectVersion::new(
        native_object_id,
        vec![AcquisitionId(acquisition_id.to_owned())],
        source.source_id.clone(),
        source.source_id.clone(),
        "fs-lvm/observation-v1".into(),
        "lvm-observation-v1".into(),
        mapping_digest(mapping_value)?,
        vec![source.clone()],
        vec![native_version],
        RevisionConsistency::Coherent,
        StorageObjectVersionKind::NativeObservation,
    )
    .map_err(|error| {
        StructureObservationError::Database(persistence_sqlite::DbError::System(error.to_string()))
    })?;
    repository.bind_version(&case_id.0, &version)?;
    Ok(())
}

fn mapping_digest(value: &serde_json::Value) -> Result<String, StructureObservationError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn normalized_vg_mapping(details: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "name": details.get("name").cloned(),
        "extentSizeSectors": details.get("extentSizeSectors").cloned(),
        "logicalVolumes": details.get("logicalVolumes").cloned(),
    })
}
