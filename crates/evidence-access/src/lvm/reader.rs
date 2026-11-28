use std::io::{Read, Seek};
use std::sync::{atomic::AtomicBool, Arc};

use evidence_core::{reader::ReaderInfo, EvidenceReader};

use crate::source_runtime::content::CONTENT_CHUNK_BYTES;

use super::plan::{LvmPlanError, LvmVolumePlan};

pub struct LvmVolumeReader {
    inner: Box<dyn EvidenceReader>,
}

impl Read for LvmVolumeReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(output)
    }
}

impl Seek for LvmVolumeReader {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl EvidenceReader for LvmVolumeReader {
    fn info(&self) -> &ReaderInfo {
        self.inner.info()
    }

    fn read_metrics(&self) -> evidence_core::reader::EvidenceReadMetrics {
        self.inner.read_metrics()
    }

    fn configure_content_scan(&mut self) {
        self.inner.configure_content_scan();
    }

    fn reserve_work(
        &self,
        bytes: u64,
    ) -> std::io::Result<Option<Arc<dyn evidence_core::reader::ReaderResourceHold>>> {
        self.inner.reserve_work(bytes)
    }

    fn preferred_read_granularity(&self) -> usize {
        self.inner.preferred_read_granularity()
    }
}

pub fn open_lvm_reader(
    plan: &LvmVolumePlan,
    cancel: &AtomicBool,
) -> Result<LvmVolumeReader, LvmPlanError> {
    check_cancel(cancel)?;
    plan.validate()?;
    plan.source_snapshot.validate()?;
    let sources = plan
        .sources
        .iter()
        .map(
            |source| crate::datasource_service::LvmPhysicalVolumeSource {
                source_path: source.source_path.clone(),
                source_kind: Some(source.source_kind.clone()),
                pv_uuid: source.pv_uuid.clone(),
                offset: source.offset,
                pv_name: None,
            },
        )
        .collect::<Vec<_>>();
    let pool = super::mapping_cache::open(
        &sources,
        |source| {
            plan.sources
                .iter()
                .find(|known| {
                    known.source_path == source.source_path && known.offset == source.offset
                })
                .map(|known| known.content.clone())
                .ok_or_else(|| std::io::Error::other("LVM physical source is missing"))
        },
        cancel,
        None,
    )?;
    let volume_group = pool.volume_group();
    if crate::datasource_service::normalize_lvm_uuid_for_match(&volume_group.id)
        != crate::datasource_service::normalize_lvm_uuid_for_match(&plan.vg_uuid)
    {
        return Err(LvmPlanError::SourceContentChanged);
    }
    let index = volume_group
        .logical_volumes
        .iter()
        .position(|volume| {
            crate::datasource_service::normalize_lvm_uuid_for_match(&volume.uuid)
                == crate::datasource_service::normalize_lvm_uuid_for_match(&plan.lv_uuid)
        })
        .ok_or(LvmPlanError::SourceContentChanged)?;
    let volume = &volume_group.logical_volumes[index];
    if volume.size_bytes != plan.logical_size
        || super::plan::segment_digest(volume_group, volume)? != plan.segment_sha256
    {
        return Err(LvmPlanError::SourceContentChanged);
    }
    check_cancel(cancel)?;
    Ok(LvmVolumeReader {
        inner: pool.open_volume(index)?,
    })
}

pub fn open_lvm_volume_reader(
    plan: &LvmVolumePlan,
    cancel: &AtomicBool,
) -> Result<Box<dyn EvidenceReader>, LvmPlanError> {
    Ok(Box::new(open_lvm_reader(plan, cancel)?))
}

pub fn verify_lvm_source_content(
    plan: &LvmVolumePlan,
    cancel: &AtomicBool,
) -> Result<(), LvmPlanError> {
    plan.validate()?;
    plan.source_snapshot.validate()?;
    for source in &plan.sources {
        source.content.verify(cancel)?;
    }
    plan.source_snapshot.validate()?;
    Ok(())
}

pub fn lvm_reader_content_bytes(plan: &LvmVolumePlan) -> u64 {
    let manifest = plan
        .sources
        .iter()
        .map(|source| {
            (source.content.chunk_sha256.len() * std::mem::size_of::<String>()) as u64
                + source
                    .content
                    .chunk_sha256
                    .iter()
                    .map(|digest| digest.len() as u64)
                    .sum::<u64>()
        })
        .sum::<u64>();
    (CONTENT_CHUNK_BYTES as u64).saturating_mul(plan.sources.len() as u64) + manifest
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), LvmPlanError> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err(LvmPlanError::Cancelled)
    } else {
        Ok(())
    }
}
