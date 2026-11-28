use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::source_runtime::content::ContentVerifiedReader;
use evidence_core::EvidenceReader;

use super::{BtrfsPoolMember, BtrfsPoolPlan, BtrfsSourceError};

pub fn open_filesystem(
    plan: &BtrfsPoolPlan,
    cancel: &AtomicBool,
) -> Result<fs_btrfs::BtrfsReader, BtrfsSourceError> {
    plan.validate()?;
    let (filesystem, identity) = mount(&plan.members, cancel)?;
    if identity != plan.identity {
        return Err(BtrfsSourceError::Identity);
    }
    Ok(filesystem)
}

pub(super) fn mount(
    members: &[BtrfsPoolMember],
    cancel: &AtomicBool,
) -> Result<(fs_btrfs::BtrfsReader, fs_btrfs::BtrfsPoolIdentity), BtrfsSourceError> {
    let mut readers = Vec::with_capacity(members.len());
    for member in members {
        if cancel.load(Ordering::Acquire) {
            return Err(BtrfsSourceError::Cancelled);
        }
        let factory = crate::source_runtime::open_factory_with_cancel(
            Path::new(&member.content.source_path),
            &member.content.source_kind,
            cancel,
        )?;
        let reader = ContentVerifiedReader::new(
            factory.open_reader_with_cancel(cancel)?,
            Arc::new(member.content.clone()),
        )?;
        let window = evidence_core::reader::PartitionWindowReader::new(
            Box::new(reader),
            member.offset,
            Some(member.length),
        )?;
        readers.push((Box::new(window) as Box<dyn EvidenceReader>, 0));
    }
    Ok(fs_btrfs::BtrfsReader::open_pool(readers)?)
}
