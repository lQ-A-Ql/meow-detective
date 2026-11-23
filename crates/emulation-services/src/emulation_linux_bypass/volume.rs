use std::io;
use std::sync::Arc;

use domain::DataSource;
use evidence_access::partition_query_service::PartitionQueryRepo as PartitionRepo;
use evidence_core::{EvidenceReader, FileSystemReader, PartitionWindowReader};
use evidence_emulation::CowDisk;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use transport::dto::EmulationLinuxBypassBlockerDto;

use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};

pub(crate) enum LinuxFilesystem {
    Ext4(Box<fs_ext4::Ext4Reader>),
    Xfs(Box<fs_xfs::XfsReader>),
}

impl LinuxFilesystem {
    pub(crate) fn supports_file_resize(&self) -> bool {
        matches!(self, Self::Xfs(_))
    }

    pub(crate) fn file_size(&self, path: &str) -> io::Result<u64> {
        match self {
            Self::Ext4(fs) => fs.file_size_by_path(path),
            Self::Xfs(fs) => fs.file_size_by_path(path),
        }
    }

    pub(crate) fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        match self {
            Self::Ext4(fs) => fs.read_file_range(path, offset, length),
            Self::Xfs(fs) => fs.read_file_range(path, offset, length),
        }
    }

    pub(crate) fn list_children(&self, path: &str) -> io::Result<Vec<evidence_core::FsNode>> {
        match self {
            Self::Ext4(fs) => fs.list_children(path),
            Self::Xfs(fs) => fs.list_children(path),
        }
    }

    pub(crate) fn verify_rewrite_state(
        &self,
        path: &str,
        expected: &str,
    ) -> Result<(), EmulationBypassError> {
        if let Self::Xfs(fs) = self {
            let plan = fs
                .plan_in_place_file_rewrite(path, expected.as_bytes())
                .map_err(map_xfs_rewrite_error)?;
            if plan.old_size != expected.len() as u64 {
                return Err(EmulationBypassError::OverlayWrite(
                    "XFS inode size does not match the edited shadow length".to_string(),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) struct LinuxPartition {
    pub(crate) fs: LinuxFilesystem,
    pub(crate) mapping: WriteMapping,
}

pub(crate) enum WriteMapping {
    Direct {
        partition_offset: u64,
        partition_length: u64,
    },
    Lvm {
        extents: Vec<fs_lvm::LvExtent>,
    },
}

impl WriteMapping {
    pub(crate) fn translate_run(
        &self,
        volume_offset: u64,
    ) -> Result<(u64, u64), EmulationBypassError> {
        match self {
            Self::Direct {
                partition_offset,
                partition_length,
            } => translate_direct(*partition_offset, *partition_length, volume_offset),
            Self::Lvm { extents } => translate_lvm(extents, volume_offset),
        }
    }
}

fn translate_direct(
    partition_offset: u64,
    partition_length: u64,
    volume_offset: u64,
) -> Result<(u64, u64), EmulationBypassError> {
    if volume_offset >= partition_length {
        return Err(EmulationBypassError::Edit(
            "write starts beyond the partition end".to_string(),
        ));
    }
    let absolute = partition_offset
        .checked_add(volume_offset)
        .ok_or_else(|| EmulationBypassError::Edit("extent address overflows".into()))?;
    Ok((absolute, partition_length - volume_offset))
}

fn translate_lvm(
    extents: &[fs_lvm::LvExtent],
    volume_offset: u64,
) -> Result<(u64, u64), EmulationBypassError> {
    let index = extents
        .partition_point(|extent| extent.logical_start <= volume_offset)
        .checked_sub(1)
        .ok_or_else(|| EmulationBypassError::Edit("offset below the LV extent map".into()))?;
    let extent = &extents[index];
    let extent_end = extent
        .logical_start
        .checked_add(extent.length)
        .ok_or_else(|| EmulationBypassError::Edit("LV extent overflows".into()))?;
    if volume_offset >= extent_end {
        return Err(EmulationBypassError::Edit(
            "offset is not covered by the LV extent map".to_string(),
        ));
    }
    let absolute = extent
        .physical_offset
        .checked_add(volume_offset - extent.logical_start)
        .ok_or_else(|| EmulationBypassError::Edit("extent address overflows".into()))?;
    Ok((absolute, extent_end - volume_offset))
}

#[derive(Clone, Copy)]
pub(crate) enum FilesystemKind {
    Ext4,
    Xfs,
}

pub(crate) fn open_linux_partition(
    context: &BypassCaseContext<'_>,
    partition_index: u32,
    overlay: Option<&Arc<CowDisk>>,
) -> Result<LinuxPartition, EmulationBypassError> {
    let (source, record) = load_partition_record(context, partition_index)?;
    let kind = filesystem_kind(&record)?;
    let count = lvm_reader_count(&record)?;
    let readers = open_readers(&source, overlay, count)?;
    if record.lvm_lv_name.is_some() {
        open_lvm_partition(readers, &record, kind)
    } else {
        open_direct_partition(readers, &record, kind)
    }
}

/// Load the data source and the partition record without opening a filesystem.
/// Used by the read-only plan/inspect paths that must classify an encrypted or
/// unsupported layout before attempting to open it.
pub(crate) fn load_partition_record(
    context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<
    (
        DataSource,
        evidence_access::partition_query_service::PartitionRecord,
    ),
    EmulationBypassError,
> {
    let source = DataSourceRepo::new(context.case_conn)
        .find_by_case(context.case_id)?
        .into_iter()
        .find(|candidate| candidate.id == *context.data_source_id)
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    let source_conn = source_kernel::source_db::open_ready_source_read_only_by_id(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
    )
    .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    let record = PartitionRepo::new(&source_conn.connection)
        .find_by_data_source_and_index(&context.data_source_id.0, partition_index as usize)?
        .ok_or(EmulationBypassError::PartitionNotFound { partition_index })?;
    Ok((source, record))
}

fn filesystem_kind(
    record: &evidence_access::partition_query_service::PartitionRecord,
) -> Result<FilesystemKind, EmulationBypassError> {
    match record_filesystem_kind(record) {
        Some(kind) => Ok(kind),
        None => Err(EmulationBypassError::Unsupported(format!(
            "partition filesystem {:?} is not supported for Linux bypass",
            record.filesystem
        ))),
    }
}

/// Classify the probed filesystem without erroring; `None` means neither Ext4
/// nor XFS (callers map that to `UnsupportedLayout` unless the volume is
/// encrypted).
pub(crate) fn record_filesystem_kind(
    record: &evidence_access::partition_query_service::PartitionRecord,
) -> Option<FilesystemKind> {
    match record.filesystem.as_deref() {
        Some("Ext4") => Some(FilesystemKind::Ext4),
        Some("XFS") => Some(FilesystemKind::Xfs),
        _ => None,
    }
}

/// A locked/encrypted volume (BitLocker today; LUKS shares the locked marker)
/// cannot be unlocked offline by the host in-place editor.
pub(crate) fn record_encrypted(
    record: &evidence_access::partition_query_service::PartitionRecord,
) -> bool {
    record.status == "locked" || record.filesystem.as_deref() == Some("BitLocker")
}

fn lvm_reader_count(
    record: &evidence_access::partition_query_service::PartitionRecord,
) -> Result<usize, EmulationBypassError> {
    if record.lvm_lv_name.is_none() {
        return Ok(1);
    }
    record
        .lvm_pv_offsets_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<u64>>(json).ok())
        .map(|offsets| offsets.len())
        .filter(|count| *count > 0)
        .ok_or_else(|| EmulationBypassError::Unsupported("LV has no persisted PV offsets".into()))
}

fn open_readers(
    source: &DataSource,
    overlay: Option<&Arc<CowDisk>>,
    count: usize,
) -> Result<Vec<Box<dyn EvidenceReader>>, EmulationBypassError> {
    (0..count)
        .map(|_| match overlay {
            Some(disk) => Ok(
                Box::new(crate::emulation_cow_reader::CowDiskReader::new(Arc::clone(
                    disk,
                ))) as Box<dyn EvidenceReader>,
            ),
            None => evidence_access::datasource_service::open_evidence_reader(
                &source.source_path,
                &source.kind,
            )
            .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string())),
        })
        .collect()
}

fn open_direct_partition(
    mut readers: Vec<Box<dyn EvidenceReader>>,
    record: &evidence_access::partition_query_service::PartitionRecord,
    kind: FilesystemKind,
) -> Result<LinuxPartition, EmulationBypassError> {
    if record.length == 0 {
        return Err(EmulationBypassError::Unsupported(
            "partition has no declared length; writes cannot be bounded".to_string(),
        ));
    }
    let window = PartitionWindowReader::new(readers.remove(0), record.offset, Some(record.length))
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    Ok(LinuxPartition {
        fs: open_filesystem(Box::new(window), kind)?,
        mapping: WriteMapping::Direct {
            partition_offset: record.offset,
            partition_length: record.length,
        },
    })
}

fn open_lvm_partition(
    readers: Vec<Box<dyn EvidenceReader>>,
    record: &evidence_access::partition_query_service::PartitionRecord,
    kind: FilesystemKind,
) -> Result<LinuxPartition, EmulationBypassError> {
    let pv_offsets: Vec<u64> = record
        .lvm_pv_offsets_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .filter(|offsets: &Vec<u64>| !offsets.is_empty())
        .ok_or_else(|| {
            EmulationBypassError::Unsupported("LV has no persisted PV offsets".into())
        })?;
    if readers.len() != pv_offsets.len() {
        return Err(EmulationBypassError::Unsupported(
            "reader count does not match the LV's PV layout".to_string(),
        ));
    }
    let pool = fs_lvm::LvmPool::discover(readers, pv_offsets)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let lv_index = pool
        .list_volumes()
        .iter()
        .position(|volume| {
            Some(volume.name.as_str()) == record.lvm_lv_name.as_deref()
                && Some(volume.uuid.as_str()) == record.lvm_lv_uuid.as_deref()
        })
        .ok_or_else(|| EmulationBypassError::PartitionNotFound {
            partition_index: record.partition_index,
        })?;
    let lv = pool
        .open_volume(lv_index)
        .map_err(|error| EmulationBypassError::Unsupported(error.to_string()))?;
    let extents = lv.extent_map().to_vec();
    Ok(LinuxPartition {
        fs: open_filesystem(Box::new(lv), kind)?,
        mapping: WriteMapping::Lvm { extents },
    })
}

fn open_filesystem(
    reader: Box<dyn EvidenceReader>,
    kind: FilesystemKind,
) -> Result<LinuxFilesystem, EmulationBypassError> {
    match kind {
        FilesystemKind::Ext4 => fs_ext4::Ext4Reader::open(reader, 0)
            .map(Box::new)
            .map(LinuxFilesystem::Ext4)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string())),
        FilesystemKind::Xfs => fs_xfs::XfsReader::open(reader, 0)
            .map(Box::new)
            .map(LinuxFilesystem::Xfs)
            .map_err(|error| EmulationBypassError::Unsupported(error.to_string())),
    }
}

/// Map a classifiable `XfsRewriteBlocker` payload to the UI-facing blocker
/// DTO. Layout-level failures collapse into `UnsupportedLayout`; only
/// `DirtyLog` and `SharedExtents` keep their own variant.
pub(crate) fn xfs_blocker_to_dto(
    blocker: fs_xfs::XfsRewriteBlocker,
) -> EmulationLinuxBypassBlockerDto {
    use fs_xfs::XfsRewriteBlocker;
    match blocker {
        XfsRewriteBlocker::DirtyLog => EmulationLinuxBypassBlockerDto::DirtyLog,
        XfsRewriteBlocker::SharedExtents => EmulationLinuxBypassBlockerDto::SharedExtents,
        XfsRewriteBlocker::RealtimeFile
        | XfsRewriteBlocker::Nrext64
        | XfsRewriteBlocker::LocalFork
        | XfsRewriteBlocker::UnwrittenExtent
        | XfsRewriteBlocker::ExtentGap
        | XfsRewriteBlocker::ExtentOverlap
        | XfsRewriteBlocker::ExtentCountMismatch
        | XfsRewriteBlocker::InsufficientAllocation
        | XfsRewriteBlocker::InvalidMetadata
        | XfsRewriteBlocker::UnsupportedInodeVersion => {
            EmulationLinuxBypassBlockerDto::UnsupportedLayout
        }
    }
}

/// Recover the typed blocker from an `io::Error` payload via `downcast_ref`,
/// without matching message substrings.
pub(crate) fn classify_xfs_blocker(error: &io::Error) -> Option<EmulationLinuxBypassBlockerDto> {
    error
        .get_ref()
        .and_then(|payload| payload.downcast_ref::<fs_xfs::XfsRewriteBlocker>())
        .copied()
        .map(xfs_blocker_to_dto)
}

/// Translate a planner `io::Error` into the service error taxonomy. Classifiable
/// blockers and `Unsupported`-kind errors remain `Unsupported`; everything else
/// is an `Edit`/read-side failure rather than a layout refusal.
pub(crate) fn map_xfs_rewrite_error(error: io::Error) -> EmulationBypassError {
    if classify_xfs_blocker(&error).is_some() || error.kind() == io::ErrorKind::Unsupported {
        EmulationBypassError::Unsupported(error.to_string())
    } else {
        EmulationBypassError::Edit(error.to_string())
    }
}
