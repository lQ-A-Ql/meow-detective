use std::collections::BTreeMap;
use std::io::SeekFrom;

use ceph_wire::{
    decode_bdev_label_block, select_bdev_label, BdevLabel, BDEV_LABEL_BLOCK_SIZE,
    BDEV_LABEL_POSITIONS,
};
use persistence_sqlite::repositories::ceph_osd_repo::{
    CephOsdInventoryRecord, CephOsdLabelReplicaRecord, CephOsdRepo,
};
use transport::CommandError;

use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe, UnsupportedImageKind,
    UnsupportedImageVolume,
};

use super::context::ImportJobContext;

pub(crate) fn persist_bluestore_probe(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    probe: &ImageFilesystemProbe,
) -> Result<(), CommandError> {
    let volume = bluestore_volume(probe)?;
    let mut reader = open_bluestore_reader(ctx, volume)?;
    let replicas = read_label_replicas(&mut *reader, volume.size_bytes)?;
    if replicas.iter().any(|replica| {
        replica.label.size == 0
            || volume
                .size_bytes
                .is_some_and(|size| replica.label.size > size)
    }) {
        return Err(CommandError::from_service_error(
            "BlueStore label reports a device size outside the mapped logical volume",
        ));
    }
    let requested_uuid = replicas
        .iter()
        .find(|replica| replica.position == 0)
        .map(|replica| replica.label.osd_uuid);
    let selection = select_bdev_label(
        replicas
            .iter()
            .map(|replica| (replica.position, replica.label.clone())),
        requested_uuid,
    )
    .map_err(|error| CommandError::from_service_error(error.to_string()))?;
    let matching_replica_count = replicas
        .iter()
        .filter(|replica| replica.label.osd_uuid == selection.label.osd_uuid)
        .count();
    let inventory_id = format!("ceph-osd:{}", selection.label.osd_uuid);
    let key_present = selection.label.metadata.contains_key("osd_key");
    let sanitized_metadata = sanitized_metadata(&selection.label.metadata, key_present);
    let label_health = label_health(&replicas, &selection);
    let inventory = inventory_record(
        data_source,
        volume,
        &InventoryBuild {
            inventory_id: &inventory_id,
            selection: &selection,
            metadata: &sanitized_metadata,
            osd_key_present: key_present,
            replica_count: matching_replica_count,
            label_health,
        },
    )?;
    let replica_records = replica_records(
        &inventory_id,
        &replicas,
        selection.label.osd_uuid,
        &selection.valid_positions,
    );
    CephOsdRepo::new(ctx.source_connection()?)
        .replace_for_data_source(
            &data_source.id.0,
            std::slice::from_ref(&inventory),
            &replica_records,
        )
        .map_err(CommandError::from_service_error)?;
    Ok(())
}

fn bluestore_volume(probe: &ImageFilesystemProbe) -> Result<&UnsupportedImageVolume, CommandError> {
    probe
        .unsupported_volumes
        .iter()
        .find(|volume| volume.kind == UnsupportedImageKind::CephBlueStore)
        .ok_or_else(|| CommandError::internal("BlueStore probe did not retain a device identity"))
}

fn open_bluestore_reader(
    ctx: &ImportJobContext<'_>,
    volume: &UnsupportedImageVolume,
) -> Result<Box<dyn evidence_core::EvidenceReader>, CommandError> {
    let candidate = ImageFilesystemCandidate {
        partition_index: None,
        partition_name: volume.name.clone(),
        kind: ImageFilesystemKind::LvmPool,
        offset: 0,
        source: volume.source,
        lvm_identity: volume.lvm_identity.clone(),
    };
    crate::import_pipeline::partition::open_candidate_reader(
        &ctx.import_config.source_path,
        &ctx.import_config.kind,
        &candidate,
    )
    .map(|(reader, _)| reader)
    .map_err(CommandError::from_service_error)
}

#[derive(Debug)]
struct LabelReplica {
    position: u64,
    label: BdevLabel,
}

fn read_label_replicas(
    reader: &mut dyn evidence_core::EvidenceReader,
    known_size: Option<u64>,
) -> Result<Vec<LabelReplica>, CommandError> {
    let device_size = known_size.unwrap_or_else(|| reader.info().size);
    let mut replicas = Vec::new();
    for position in BDEV_LABEL_POSITIONS {
        if position.saturating_add(BDEV_LABEL_BLOCK_SIZE as u64) > device_size {
            continue;
        }
        reader
            .seek(SeekFrom::Start(position))
            .map_err(CommandError::from_service_error)?;
        let mut block = vec![0; BDEV_LABEL_BLOCK_SIZE];
        reader
            .read_exact(&mut block)
            .map_err(CommandError::from_service_error)?;
        if let Ok(label) = decode_bdev_label_block(&block) {
            replicas.push(LabelReplica { position, label });
        }
    }
    if replicas.is_empty() {
        return Err(CommandError::from_service_error(
            "BlueStore signature was detected but no complete CRC-valid label could be decoded",
        ));
    }
    Ok(replicas)
}

fn inventory_record(
    data_source: &domain::DataSource,
    volume: &UnsupportedImageVolume,
    build: &InventoryBuild<'_>,
) -> Result<CephOsdInventoryRecord, CommandError> {
    let identity = volume.lvm_identity.as_ref();
    Ok(CephOsdInventoryRecord {
        id: build.inventory_id.to_string(),
        data_source_id: data_source.id.0.clone(),
        partition_index: None,
        lvm_vg_uuid: identity.map(|value| value.vg_uuid.clone()),
        lvm_vg_name: identity.map(|value| value.vg_name.clone()),
        lvm_lv_uuid: identity.map(|value| value.lv_uuid.clone()),
        lvm_lv_name: identity.map(|value| value.lv_name.clone()),
        osd_uuid: build.selection.label.osd_uuid.to_string(),
        ceph_fsid: build.metadata.get("ceph_fsid").cloned(),
        whoami: parse_optional(build.metadata, "whoami")?,
        device_role: build
            .metadata
            .get("type")
            .cloned()
            .unwrap_or_else(|| "bluestore".to_string()),
        device_size: build.selection.label.size,
        birth_time_seconds: i64::from(build.selection.label.birth_time.seconds),
        birth_time_nanoseconds: build.selection.label.birth_time.nanoseconds,
        description: build.selection.label.description.clone(),
        is_multi: build.selection.is_multi,
        selected_epoch: build.selection.epoch,
        valid_label_count: build.replica_count as u32,
        label_health: build.label_health.clone(),
        osd_key_present: build.osd_key_present,
        kv_backend: build.metadata.get("kv_backend").cloned(),
        bluefs_enabled: parse_bool(build.metadata.get("bluefs")),
        ceph_version_when_created: build.metadata.get("ceph_version_when_created").cloned(),
        require_osd_release: parse_optional(build.metadata, "require_osd_release")?,
        sanitized_metadata_json: serde_json::to_string(build.metadata)
            .map_err(CommandError::from_service_error)?,
    })
}

struct InventoryBuild<'a> {
    inventory_id: &'a str,
    selection: &'a ceph_wire::BdevLabelSelection,
    metadata: &'a BTreeMap<String, String>,
    osd_key_present: bool,
    replica_count: usize,
    label_health: String,
}

fn replica_records(
    inventory_id: &str,
    replicas: &[LabelReplica],
    osd_uuid: uuid::Uuid,
    selected_positions: &[u64],
) -> Vec<CephOsdLabelReplicaRecord> {
    replicas
        .iter()
        .filter(|replica| replica.label.osd_uuid == osd_uuid)
        .map(|replica| CephOsdLabelReplicaRecord {
            inventory_id: inventory_id.to_string(),
            position: replica.position,
            device_size: replica.label.size,
            birth_time_seconds: i64::from(replica.label.birth_time.seconds),
            birth_time_nanoseconds: replica.label.birth_time.nanoseconds,
            description: replica.label.description.clone(),
            is_multi: replica.label.is_multi(),
            epoch: replica.label.epoch().ok().flatten(),
            is_selected: selected_positions.contains(&replica.position),
            struct_version: replica.label.struct_version,
            struct_compat_version: replica.label.struct_compat_version,
        })
        .collect()
}

fn sanitized_metadata(
    metadata: &BTreeMap<String, String>,
    osd_key_present: bool,
) -> BTreeMap<String, String> {
    const ALLOWED_KEYS: &[&str] = &[
        "bluefs",
        "ceph_fsid",
        "ceph_version_when_created",
        "created_at",
        "epoch",
        "kv_backend",
        "magic",
        "mkfs_done",
        "multi",
        "ready",
        "require_osd_release",
        "type",
        "whoami",
    ];
    let mut sanitized = metadata
        .iter()
        .filter(|(key, _)| ALLOWED_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    sanitized.insert("osd_key_present".to_string(), osd_key_present.to_string());
    sanitized
}

fn label_health(replicas: &[LabelReplica], selection: &ceph_wire::BdevLabelSelection) -> String {
    let matching = replicas
        .iter()
        .filter(|replica| replica.label.osd_uuid == selection.label.osd_uuid)
        .count();
    if selection.valid_positions.len() == matching && matching > 1 {
        "healthy".to_string()
    } else if matching > 1 {
        "staleReplica".to_string()
    } else {
        "singleReplica".to_string()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph.rs"]
mod tests;

fn parse_optional<T>(
    metadata: &BTreeMap<String, String>,
    key: &'static str,
) -> Result<Option<T>, CommandError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    metadata
        .get(key)
        .map(|value| {
            value.parse().map_err(|error| {
                CommandError::from_service_error(format!(
                    "invalid BlueStore metadata field {key}: {error}"
                ))
            })
        })
        .transpose()
}

fn parse_bool(value: Option<&String>) -> Option<bool> {
    value.map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}
