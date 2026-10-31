use std::path::Path;

use crate::datasource_service::LvmDiscoverySource;

use super::LinuxEvidenceSetMemberPlan;

pub(super) fn lvm_extra_sources(
    members: &[LinuxEvidenceSetMemberPlan],
    primary_path: &Path,
) -> Vec<LvmDiscoverySource> {
    members
        .iter()
        .filter(|member| member.source_path != primary_path)
        .map(|member| LvmDiscoverySource::new(&member.source_path, member.source_kind.clone()))
        .collect()
}
