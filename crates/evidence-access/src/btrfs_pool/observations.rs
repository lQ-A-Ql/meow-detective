use super::BtrfsPoolPlan;
use crate::datasource_service::LvmDiscoverySource;
use domain::structure::{NativeObservation, NativeVersion, ObservationLocation};

pub(crate) fn append_observations(
    sources: &[LvmDiscoverySource],
    pools: &[BtrfsPoolPlan],
    observations: &mut [Vec<NativeObservation>],
) {
    for (source, facts) in sources.iter().zip(observations) {
        for pool in pools {
            for member in &pool.members {
                if member.content.source_kind != source.source_kind
                    || std::path::Path::new(&member.content.source_path) != source.source_path
                {
                    continue;
                }
                facts.push(NativeObservation {
                    native_identity: format!(
                        "btrfs:{}:{}",
                        member.identity.fsid, member.identity.devid
                    ),
                    version: NativeVersion::Btrfs {
                        fsid: member.identity.fsid.clone(),
                        generation: member.identity.generation,
                        device_count: member.identity.device_count,
                        mapping_sha256: pool.identity.mapping_sha256.clone(),
                    },
                    location: ObservationLocation::SourceWindow {
                        byte_offset: member.offset,
                        byte_length: member.length,
                    },
                    parser_version: super::ADAPTER_VERSION.into(),
                    native_checksum_verified: true,
                    details: serde_json::json!({
                        "device": member.identity, "profiles": pool.identity.profiles,
                        "mappingSha256": pool.identity.mapping_sha256,
                    }),
                });
            }
        }
    }
}
