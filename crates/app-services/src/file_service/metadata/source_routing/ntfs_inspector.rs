//! NTFS technical inspector use case.

use std::path::Path;
use std::sync::Arc;

use domain::CaseId;
use fs_ntfs::{ForensicDataRun, NtfsReader, NtfsTechnicalAttribute, NtfsTechnicalFile};
use rusqlite::Connection;
use transport::dto::{ForensicDataRunDto, NtfsTechnicalAttributeDto, NtfsTechnicalFileDto};

use crate::file_service::{
    metadata::source_routing::open_source_for_file_id,
    viewer::{descriptor_for_file_with_cache, PreviewReadContext},
    FileServiceError, SourceReadContext,
};

pub fn inspect_ntfs_file_for_case(
    bitlocker_runtime: &Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<NtfsTechnicalFileDto, FileServiceError> {
    let (global, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut context = SourceReadContext::new(
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &global.data_source_id,
    )
    .with_bitlocker_runtime(bitlocker_runtime.clone());
    let descriptor = descriptor_for_file_with_cache(&mut context, &global.local_id)?;
    let candidate = descriptor.partition_candidates.first().ok_or_else(|| {
        FileServiceError::Unsupported(
            "NTFS inspection requires an image-backed NTFS file".to_string(),
        )
    })?;
    if !candidate.filesystem_kind.eq_ignore_ascii_case("NTFS") {
        return Err(FileServiceError::Unsupported(
            "file is not stored on an NTFS partition".to_string(),
        ));
    }
    let (reader, offset, filesystem) =
        context.open_candidate_block_reader(&descriptor, candidate)?;
    if !filesystem.eq_ignore_ascii_case("NTFS") {
        return Err(FileServiceError::Unsupported(
            "routed partition is not NTFS".to_string(),
        ));
    }
    let ntfs = NtfsReader::open(reader, offset).map_err(FileServiceError::Io)?;
    let inode = parse_mft_inode(&global.local_id.0, candidate.partition_index)?;
    ntfs.inspect_file_by_inode(inode)
        .map(convert_file)
        .map_err(FileServiceError::Io)
}

fn parse_mft_inode(value: &str, partition_index: usize) -> Result<u64, FileServiceError> {
    let mut parts = value.split(':');
    if parts.next() != Some("mft") {
        return Err(FileServiceError::Unsupported(
            "file is not an NTFS MFT entry".to_string(),
        ));
    }
    let numbers = parts.collect::<Vec<_>>();
    let inode = match numbers.as_slice() {
        [record] => *record,
        [partition, record] if partition.parse::<usize>().ok() == Some(partition_index) => *record,
        _ => {
            return Err(FileServiceError::security(
                "MFT file identifier does not match the routed partition",
            ))
        }
    };
    inode
        .parse::<u64>()
        .map_err(|_| FileServiceError::security("MFT file identifier is invalid".to_string()))
}

fn convert_file(file: NtfsTechnicalFile) -> NtfsTechnicalFileDto {
    NtfsTechnicalFileDto {
        inode: file.inode,
        sequence_number: file.sequence_number,
        flags: file.flags,
        parent_reference: file.parent_reference,
        record_offset: file.record_offset,
        record_size: file.record_size,
        record_raw: file.record_raw,
        attributes: file.attributes.into_iter().map(convert_attribute).collect(),
    }
}

fn convert_attribute(attribute: NtfsTechnicalAttribute) -> NtfsTechnicalAttributeDto {
    NtfsTechnicalAttributeDto {
        attribute_type: attribute.attribute_type,
        name: attribute.name,
        instance: attribute.instance,
        non_resident: attribute.non_resident,
        allocated_size: attribute.allocated_size,
        real_size: attribute.real_size,
        initialized_size: attribute.initialized_size,
        raw: attribute.raw,
        data_runs: attribute.data_runs.into_iter().map(convert_run).collect(),
    }
}

fn convert_run(run: ForensicDataRun) -> ForensicDataRunDto {
    ForensicDataRunDto {
        header: run.header,
        length_field_size: run.length_field_size,
        offset_field_size: run.offset_field_size,
        cluster_count: run.cluster_count,
        relative_lcn: run.relative_lcn,
        absolute_lcn: run.absolute_lcn,
        logical_offset: run.logical_offset,
        raw: run.raw,
        physical_offset: run.physical_offset,
    }
}
