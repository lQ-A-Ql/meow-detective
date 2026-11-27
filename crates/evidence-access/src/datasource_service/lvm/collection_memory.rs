use super::super::{ImageFilesystemProbe, LvmDiscoverySource, LvmLogicalVolumeIdentity};
use crate::source_runtime::snapshot::SourceSnapshot;
use source_kernel::resource_budget::{ResourcePermit, Resources, MIB};
use std::sync::Arc;

pub(super) const PREPARATION_BYTES: u64 = 192 * MIB;
pub(super) const RETAINED_BYTES: u64 = 16 * MIB;

pub(super) struct PlanMemory {
    _permit: ResourcePermit,
    bytes: u64,
}
impl std::fmt::Debug for PlanMemory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlanMemory")
            .field("bytes", &self.bytes)
            .finish()
    }
}
impl PartialEq for PlanMemory {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}
impl Eq for PlanMemory {}

pub(super) fn retain(
    permit: ResourcePermit,
    sources: &[LvmDiscoverySource],
    probes: &[ImageFilesystemProbe],
    snapshot: &SourceSnapshot,
    md_members: &[crate::mdraid::MdRaidMemberPlan],
) -> std::io::Result<Arc<PlanMemory>> {
    let mut bytes = snapshot.metadata_bytes() + 4096;
    bytes += md_members
        .iter()
        .map(|member| {
            (std::mem::size_of_val(member)
                + member.superblock.dev_roles.capacity() * 2
                + member.superblock.name.capacity()) as u64
                + match &member.origin {
                    crate::mdraid::MdRaidMemberOrigin::Direct { source_path, .. } => {
                        source_path.capacity() as u64
                    }
                    crate::mdraid::MdRaidMemberOrigin::Lvm {
                        vg_name,
                        vg_uuid,
                        lv_name,
                        lv_uuid,
                        identity,
                    } => {
                        (vg_name.capacity()
                            + vg_uuid.capacity()
                            + lv_name.capacity()
                            + lv_uuid.capacity()) as u64
                            + identity_bytes(Some(identity))
                    }
                }
        })
        .sum::<u64>();
    bytes += sources
        .iter()
        .map(|source| std::mem::size_of_val(source) as u64 + source.source_path.capacity() as u64)
        .sum::<u64>();
    for probe in probes {
        bytes += std::mem::size_of_val(probe) as u64;
        bytes += probe
            .warnings
            .iter()
            .map(|value| value.capacity() as u64 + 24)
            .sum::<u64>();
        bytes += probe
            .gpt_disk_guid
            .as_ref()
            .map_or(0, |value| value.capacity() as u64);
        for candidate in &probe.candidates {
            bytes += std::mem::size_of_val(candidate) as u64;
            bytes += candidate
                .partition_name
                .as_ref()
                .map_or(0, |value| value.capacity() as u64);
            bytes += identity_bytes(candidate.lvm_identity.as_ref());
        }
        for partition in &probe.partitions {
            bytes += std::mem::size_of_val(partition) as u64
                + (partition.name.capacity() + partition.kind_label.capacity()) as u64;
            for value in [
                &partition.type_guid,
                &partition.gpt_partition_uuid,
                &partition.filesystem_uuid,
            ] {
                bytes += value.as_ref().map_or(0, |value| value.capacity() as u64);
            }
            bytes += identity_bytes(partition.lvm_identity.as_ref());
        }
        for volume in &probe.unsupported_volumes {
            bytes += std::mem::size_of_val(volume) as u64;
            bytes += volume
                .name
                .as_ref()
                .map_or(0, |value| value.capacity() as u64);
            bytes += identity_bytes(volume.lvm_identity.as_ref());
        }
    }
    // Retain allocator headroom for vector capacity, strings, and shared ownership.
    bytes = bytes.saturating_mul(2).max(65536);
    if bytes > RETAINED_BYTES {
        return Err(source_kernel::resource_budget::ResourceError::Limit.into_io());
    }
    let permit = permit
        .shrink(Resources {
            bytes,
            metadata: bytes,
            ..Default::default()
        })
        .map_err(source_kernel::resource_budget::ResourceError::into_io)?;
    Ok(Arc::new(PlanMemory {
        _permit: permit,
        bytes,
    }))
}

fn identity_bytes(identity: Option<&LvmLogicalVolumeIdentity>) -> u64 {
    let Some(identity) = identity else {
        return 0;
    };
    let mut bytes = (identity.vg_uuid.capacity()
        + identity.vg_name.capacity()
        + identity.lv_uuid.capacity()
        + identity.lv_name.capacity()
        + identity.pv_offsets.capacity() * 8) as u64;
    for source in &identity.pv_sources {
        bytes += std::mem::size_of_val(source) as u64
            + (source.source_path.capacity() + source.pv_uuid.capacity()) as u64;
        bytes += source
            .pv_name
            .as_ref()
            .map_or(0, |name| name.capacity() as u64);
    }
    bytes
}
