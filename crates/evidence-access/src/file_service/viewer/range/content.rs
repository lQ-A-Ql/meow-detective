use std::io::Read;

use domain::FileEntry;
use persistence_source::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use super::{archive, logical};
use crate::file_service::{
    viewer::{
        exact_partition_candidate, open_descriptor_image_file,
        open_descriptor_image_file_with_context, open_e01_file, open_e01_reader_cached,
        open_mdraid_file, open_mdraid_file_seekable, resolve_partition_index_for_entry,
        validate_readable_file_entry, PreviewDescriptor, PreviewReadContext, RangeContentReader,
    },
    FileServiceError,
};

pub(crate) fn open_file_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader = match descriptor.source_kind.as_str() {
        "logical_directory" => logical::open_descriptor_file(descriptor),
        "logical_archive" => archive::open_descriptor_file(descriptor),
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" | "local_disk" | "android_sparse" => open_raw_descriptor_file(descriptor),
        "md_raid" => open_mdraid_descriptor_file(descriptor),
        other => unsupported_source(other),
    }?;
    Ok(match reader {
        RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
        RangeContentReader::Streaming(reader) => reader,
    })
}

pub(crate) fn open_file_content_for_descriptor_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError>
where
    C: PreviewReadContext,
{
    if context.requires_bound_reads() {
        return Ok(
            match open_descriptor_image_file_with_context(context, descriptor)? {
                RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
                RangeContentReader::Streaming(reader) => reader,
            },
        );
    }
    if matches!(
        descriptor.source_kind.as_str(),
        "e01" | "raw" | "local_disk" | "android_sparse"
    ) && !context.is_bitlocker_candidate(exact_partition_candidate(descriptor)?)?
    {
        return open_file_content_for_descriptor(descriptor);
    }
    if !matches!(descriptor.source_kind.as_str(), "ceph_rbd" | "nested_range")
        && !matches!(
            descriptor.source_kind.as_str(),
            "e01" | "raw" | "local_disk" | "android_sparse"
        )
    {
        return open_file_content_for_descriptor(descriptor);
    }
    Ok(
        match open_descriptor_image_file_with_context(context, descriptor)? {
            RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
            RangeContentReader::Streaming(reader) => reader,
        },
    )
}

pub(crate) fn open_range_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    match descriptor.source_kind.as_str() {
        "logical_directory" => {
            logical::open_descriptor_seekable(descriptor).map(RangeContentReader::Seekable)
        }
        "logical_archive" => {
            archive::open_descriptor_seekable(descriptor).map(RangeContentReader::Seekable)
        }
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" | "local_disk" | "android_sparse" => open_raw_descriptor_file(descriptor),
        "md_raid" => open_mdraid_descriptor_file(descriptor),
        other => unsupported_source(other),
    }
}

pub(crate) fn open_range_content_for_descriptor_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError>
where
    C: PreviewReadContext,
{
    if context.requires_bound_reads() {
        return open_descriptor_image_file_with_context(context, descriptor);
    }
    if matches!(
        descriptor.source_kind.as_str(),
        "e01" | "raw" | "local_disk" | "android_sparse"
    ) && !context.is_bitlocker_candidate(exact_partition_candidate(descriptor)?)?
    {
        return open_range_content_for_descriptor(descriptor);
    }
    if !matches!(descriptor.source_kind.as_str(), "ceph_rbd" | "nested_range")
        && !matches!(
            descriptor.source_kind.as_str(),
            "e01" | "raw" | "local_disk" | "android_sparse"
        )
    {
        return open_range_content_for_descriptor(descriptor);
    }
    open_descriptor_image_file_with_context(context, descriptor)
}

fn open_e01_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let case_id = descriptor.case_id.clone();
    open_descriptor_image_file(descriptor, move |source_path| {
        open_e01_reader_cached(source_path, &case_id)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

fn open_raw_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    open_descriptor_image_file(descriptor, |source_path| {
        match descriptor.source_kind.as_str() {
            "local_disk" => evidence_core::LocalDiskReader::open(source_path)
                .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
            "android_sparse" => image_android::AndroidSparseReader::open(source_path)
                .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                .map_err(std::io::Error::other),
            _ => evidence_core::RawImageReader::open(source_path)
                .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>),
        }
    })
}

pub(crate) fn open_file_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    validate_readable_file_entry(conn, entry)?;
    let (kind, source_path) = source_location(repo, entry)?;
    match kind.as_str() {
        "logical_directory" => logical::open_file(&source_path, entry),
        "logical_archive" => archive::open_file(&source_path, entry),
        "e01" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_e01_file(conn, &source_path, entry, partition_index)
        }
        "raw" | "local_disk" | "android_sparse" => {
            let descriptor =
                crate::file_service::viewer::preview_descriptor_for_case(conn, "", &entry.id)?;
            open_file_content_for_descriptor(&descriptor)
        }
        "md_raid" => open_mdraid_file(
            &entry.data_source_id.0,
            &source_path,
            entry,
            resolve_partition_index_for_entry(repo, entry)?,
        ),
        other => unsupported_source(other),
    }
}

pub(crate) fn open_range_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<RangeContentReader, FileServiceError> {
    validate_readable_file_entry(conn, entry)?;
    let (kind, source_path) = source_location(repo, entry)?;
    match kind.as_str() {
        "logical_directory" => {
            logical::open_file_seekable(&source_path, entry).map(RangeContentReader::Seekable)
        }
        "logical_archive" => {
            archive::open_file_seekable(&source_path, entry).map(RangeContentReader::Seekable)
        }
        "e01" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_e01_file(conn, &source_path, entry, partition_index)
                .map(RangeContentReader::Streaming)
        }
        "raw" | "local_disk" | "android_sparse" => {
            let descriptor =
                crate::file_service::viewer::preview_descriptor_for_case(conn, "", &entry.id)?;
            open_raw_descriptor_file(&descriptor)
        }
        "md_raid" => open_mdraid_file_seekable(
            &entry.data_source_id.0,
            &source_path,
            entry,
            resolve_partition_index_for_entry(repo, entry)?,
        )
        .map(RangeContentReader::Seekable),
        other => unsupported_source(other),
    }
}

fn source_location(
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<(String, String), FileServiceError> {
    repo.find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))
}

fn unsupported_source<T>(kind: &str) -> Result<T, FileServiceError> {
    Err(FileServiceError::other(format!(
        "Range reading is not yet wired for data source kind '{kind}'"
    )))
}

fn open_mdraid_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let entry = crate::file_service::viewer::descriptor_file_entry(descriptor);
    open_mdraid_file_seekable(
        &descriptor.data_source_id,
        &descriptor.source_path,
        &entry,
        descriptor.partition_index,
    )
        .map(RangeContentReader::Seekable)
}
