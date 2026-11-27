use std::io::{Read, Seek};
use std::sync::{atomic::AtomicBool, Arc};

use evidence_core::{reader::ReaderInfo, EvidenceReader};

use crate::source_runtime::content::{ContentVerifiedReader, CONTENT_CHUNK_BYTES};

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
    let mut readers = Vec::with_capacity(plan.sources.len());
    for source in &plan.sources {
        let reader = crate::datasource_service::open_evidence_reader(
            std::path::Path::new(&source.source_path),
            &source.source_kind,
        )?;
        readers.push(Box::new(ContentVerifiedReader::new(
            reader,
            Arc::new(source.content.clone()),
        )?) as Box<dyn EvidenceReader>);
    }
    check_cancel(cancel)?;
    let offsets = plan.sources.iter().map(|source| source.offset).collect();
    let pool = fs_lvm::LvmPool::discover(readers, offsets)?;
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
        inner: pool.open_volume_reader(index)?,
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
