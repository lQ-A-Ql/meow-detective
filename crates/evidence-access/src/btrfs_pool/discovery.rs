use std::{collections::BTreeMap, sync::atomic::AtomicBool};

use super::{BtrfsPoolMember, BtrfsPoolPlan, BtrfsSourceError, ADAPTER_VERSION, POLICY_VERSION};
use crate::{
    datasource_service::{ImageFilesystemKind, ImageFilesystemProbe, LvmDiscoverySource},
    source_runtime::content::SourceContent,
};

pub fn discover(
    sources: &[LvmDiscoverySource],
    probes: &mut [ImageFilesystemProbe],
    content: impl Fn(&LvmDiscoverySource) -> Result<SourceContent, BtrfsSourceError>,
    cancel: &AtomicBool,
) -> Result<Vec<BtrfsPoolPlan>, BtrfsSourceError> {
    let mut groups = BTreeMap::<String, Vec<BtrfsPoolMember>>::new();
    for (source, probe) in sources.iter().zip(probes.iter_mut()) {
        let mut pooled = Vec::new();
        for (index, candidate) in probe.candidates.iter().enumerate() {
            if candidate.kind != ImageFilesystemKind::Btrfs {
                continue;
            }
            let factory = crate::source_runtime::open_factory_with_cancel(
                &source.source_path,
                &source.source_kind,
                cancel,
            )?;
            let identity = fs_btrfs::inspect_device(
                factory.open_reader_with_cancel(cancel)?,
                candidate.offset,
            )?;
            if identity.device_count < 2 {
                continue;
            }
            let member = BtrfsPoolMember {
                offset: candidate.offset,
                length: candidate.length.unwrap_or(identity.device_size),
                identity: identity.clone(),
                content: content(source)?,
            };
            groups.entry(identity.fsid).or_default().push(member);
            pooled.push(index);
        }
        for index in pooled.into_iter().rev() {
            probe.candidates.remove(index);
        }
    }
    groups
        .into_values()
        .map(|mut members| {
            members.sort_by_key(|member| member.identity.devid);
            let (_, identity) = super::reader::mount(&members, cancel)?;
            let plan = BtrfsPoolPlan {
                adapter_version: ADAPTER_VERSION.into(),
                policy_version: POLICY_VERSION.into(),
                identity,
                members,
            };
            plan.validate()?;
            Ok(plan)
        })
        .collect()
}
