use std::{path::Path, sync::Arc};

use persistence_sqlite::repositories::partition_repo::PartitionRepo;

use crate::file_service::{
    viewer::{PreviewDescriptor, PreviewPartitionCandidate, PreviewReadContext},
    FileServiceError,
};

use super::SourceReadContext;

impl<'a> SourceReadContext<'a> {
    pub(crate) fn with_bitlocker_runtime(
        mut self,
        runtime: Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    ) -> Self {
        self.bitlocker_runtime = Some(runtime);
        self
    }
}

pub(super) fn open_candidate_block_reader(
    context: &mut SourceReadContext<'_>,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
) -> Result<(Box<dyn evidence_core::EvidenceReader>, u64, String), FileServiceError> {
    let source_path = Path::new(&descriptor.source_path);
    let mut lvm_cache = crate::file_service::viewer::LvmPoolRequestCache::new();
    let mut open_reader = |_: &Path| {
        context
            .open_evidence_reader(descriptor)
            .map_err(|error| std::io::Error::other(error.to_string()))
    };
    let (reader, filesystem_offset) =
        crate::file_service::viewer::open_candidate_block_reader_with_lvm_cache(
            source_path,
            candidate,
            &mut open_reader,
            &mut lvm_cache,
        )
        .map_err(FileServiceError::Io)?;
    let Some(partition) = PartitionRepo::new(context.source_conn)
        .find_by_data_source_and_index(&context.data_source_id.0, candidate.partition_index)?
    else {
        return Err(FileServiceError::not_found(format!(
            "Partition {} metadata is missing",
            candidate.partition_index
        )));
    };
    if !crate::partition_capabilities::is_bitlocker_partition(&partition) {
        return Ok((reader, filesystem_offset, candidate.filesystem_kind.clone()));
    }
    let Some(runtime) = context.bitlocker_runtime.as_ref() else {
        return Err(FileServiceError::Unsupported(
            "BitLocker volume is locked; register a verified unlock first".to_string(),
        ));
    };
    // Direct volumes have no partition-table length and are persisted as zero.
    // Passing Some(0) would make the whole decrypted volume appear empty.
    let partition_length = (partition.length > 0).then_some(partition.length);
    let mut plaintext = crate::bitlocker_runtime::open_registered_bitlocker_volume(
        reader,
        filesystem_offset,
        partition_length,
        &context.case_id.0,
        &context.data_source_id.0,
        candidate.partition_index,
        runtime,
    )
    .map_err(map_bitlocker_runtime_error)?;
    let filesystem_kind = detect_plaintext_filesystem(plaintext.as_mut())?;
    Ok((plaintext, 0, filesystem_kind))
}

pub(super) fn is_bitlocker_candidate(
    context: &SourceReadContext<'_>,
    candidate: &PreviewPartitionCandidate,
) -> Result<bool, FileServiceError> {
    Ok(PartitionRepo::new(context.source_conn)
        .find_by_data_source_and_index(&context.data_source_id.0, candidate.partition_index)?
        .is_some_and(|partition| crate::partition_capabilities::is_bitlocker_partition(&partition)))
}

pub(super) fn detect_plaintext_filesystem(
    reader: &mut dyn evidence_core::EvidenceReader,
) -> Result<String, FileServiceError> {
    match crate::datasource_service::read_boot_filesystem(reader, 0).map_err(|error| {
        FileServiceError::other(format!("BitLocker plaintext probe failed: {error}"))
    })? {
        Some(crate::datasource_service::ImageFilesystemKind::Ntfs) => Ok("NTFS".to_string()),
        Some(crate::datasource_service::ImageFilesystemKind::Fat) => Ok("FAT".to_string()),
        Some(crate::datasource_service::ImageFilesystemKind::BitLocker) => {
            Err(FileServiceError::Unsupported(
                "BitLocker plaintext probe still reports an encrypted volume".to_string(),
            ))
        }
        Some(kind) => Err(FileServiceError::Unsupported(format!(
            "BitLocker plaintext filesystem '{kind:?}' is not supported"
        ))),
        None if crate::file_service::viewer::looks_like_exfat_boot_sector(reader, 0)? => {
            Ok("EXFAT".to_string())
        }
        None => Err(FileServiceError::Unsupported(
            "BitLocker plaintext filesystem could not be identified".to_string(),
        )),
    }
}

fn map_bitlocker_runtime_error(
    error: crate::bitlocker_runtime::BitLockerRuntimeError,
) -> FileServiceError {
    match error {
        crate::bitlocker_runtime::BitLockerRuntimeError::Locked => {
            FileServiceError::Unsupported("BitLocker volume is locked".to_string())
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::RegistryUnavailable => {
            FileServiceError::other("BitLocker runtime registry is unavailable")
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::InvalidWindow(error) => {
            FileServiceError::Io(error)
        }
        crate::bitlocker_runtime::BitLockerRuntimeError::Volume(error) => {
            FileServiceError::other(format!("BitLocker volume open failed: {error}"))
        }
    }
}
