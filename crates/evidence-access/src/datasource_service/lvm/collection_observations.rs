use super::{collection::CollectionPlanError, model::LvmPvDiscoveryInfo};
use crate::{
    datasource_service::LvmDiscoverySource,
    mdraid::{MdRaidMemberOrigin, MdRaidMemberPlan},
};
use domain::structure::{NativeObservation, NativeVersion, ObservationLocation};

pub(super) fn collect(
    sources: &[LvmDiscoverySource],
    pvs: &[LvmPvDiscoveryInfo],
    md_members: &[MdRaidMemberPlan],
) -> Result<Vec<Vec<NativeObservation>>, CollectionPlanError> {
    sources
        .iter()
        .map(|source| {
            let mut facts = Vec::new();
            for pv in pvs.iter().filter(|pv| {
                std::path::Path::new(&pv.source.source_path) == source.source_path
                    && pv.source.source_kind.as_ref() == Some(&source.source_kind)
            }) {
                facts.extend(pv_observations(pv));
            }
            for member in md_members {
                if let MdRaidMemberOrigin::Lvm { identity, vg_uuid, lv_uuid, .. } = &member.origin {
                    if identity.pv_sources.iter().any(|pv| std::path::Path::new(&pv.source_path) == source.source_path) {
                        facts.push(NativeObservation {
                            native_identity: format!("md:{}:{}", hex::encode(member.superblock.array_uuid), member.superblock.dev_number),
                            version: NativeVersion::Md {
                                metadata_version: format!("{:?}", member.superblock.metadata_version),
                                events: member.superblock.events,
                            },
                            location: ObservationLocation::Unavailable {reason: "MD metadata is addressed through an LVM logical view".into()},
                            parser_version: "fs-lvm/mdraid-v2".into(),
                            native_checksum_verified: member.superblock.checksum_valid,
                            details: serde_json::json!({"vgUuid": vg_uuid, "lvUuid": lv_uuid, "superblock": member.superblock}),
                        });
                    }
                }
                if let MdRaidMemberOrigin::Direct {
                    source_path,
                    source_kind,
                    byte_offset,
                    byte_length,
                } = &member.origin
                {
                    if std::path::Path::new(source_path) == source.source_path
                        && *source_kind == source.source_kind
                    {
                        facts.push(NativeObservation {
                            native_identity: format!(
                                "md:{}:{}",
                                hex::encode(member.superblock.array_uuid),
                                member.superblock.dev_number
                            ),
                            version: NativeVersion::Md {
                                metadata_version: format!(
                                    "{:?}",
                                    member.superblock.metadata_version
                                ),
                                events: member.superblock.events,
                            },
                            location: ObservationLocation::SourceWindow {
                                byte_offset: *byte_offset,
                                byte_length: *byte_length,
                            },
                            parser_version: "fs-lvm/mdraid-v2".into(),
                            native_checksum_verified: member.superblock.checksum_valid,
                            details: serde_json::to_value(&member.superblock)
                                .map_err(std::io::Error::other)?,
                        });
                    }
                }
            }
            Ok(facts)
        })
        .collect()
}

fn pv_observations(pv: &LvmPvDiscoveryInfo) -> Vec<NativeObservation> {
    let mut facts = vec![NativeObservation {
        native_identity: format!("lvm-pv:{}", pv.label.pv_uuid),
        version: NativeVersion::LvmLabel,
        location: ObservationLocation::SourceWindow {
            byte_offset: pv.source.offset,
            byte_length: 2048,
        },
        parser_version: "fs-lvm/label-v1".into(),
        native_checksum_verified: true,
        details: serde_json::json!({"pvSize": pv.label.pv_size}),
    }];
    if let Some(vg) = &pv.volume_group {
        // The parser selects a committed copy across areas without exposing its exact text extent.
        let location = ObservationLocation::Unavailable {
            reason: "committed metadata parser does not expose the selected copy extent".into(),
        };
        let volume_details = vg.logical_volumes.iter().map(|lv| serde_json::json!({
            "uuid": lv.uuid, "name": lv.name, "sizeBytes": lv.size_bytes, "role": lv.role.as_str(),
        })).collect::<Vec<_>>();
        facts.push(NativeObservation {
            native_identity: format!("lvm-vg:{}", vg.id),
            version: NativeVersion::LvmCommitted { vg_uuid: vg.id.clone(), seqno: vg.seqno },
            location,
            parser_version: "fs-lvm/committed-metadata-v2".into(),
            native_checksum_verified: true,
            details: serde_json::json!({
                "name": vg.name, "extentSizeSectors": vg.extent_size, "logicalVolumes": volume_details,
                "metadataAreas": pv.label.metadata_areas.iter().map(|area| serde_json::json!({
                    "pvRelativeOffset": area.offset, "length": area.size,
                })).collect::<Vec<_>>()
            }),
        });
    }
    facts
}
