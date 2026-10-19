use crate::{
    build_segment_path,
    geometry::{find_geometry, E01Geometry},
    open_limits::{OpenBudget, MAX_METADATA_BYTES, MAX_SEGMENTS},
    paged_index::PagedIndex,
    sections::{self, SegmentSections},
    E01Reader,
};
use evidence_core::ReaderInfo;
use std::{fs::File, io, path::Path, sync::atomic::AtomicBool};

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with_segment_limit(path, MAX_SEGMENTS)
    }
    /// Reject segment growth before opening more handles than the caller reserved.
    pub fn open_with_segment_limit(path: &Path, max_segments: u32) -> io::Result<Self> {
        Self::open_with_segment_limit_cancellable(path, max_segments, None)
    }
    /// Validate metadata with bounded allocations and a cancellable section/table walk.
    pub fn open_with_segment_limit_cancellable(
        path: &Path,
        max_segments: u32,
        cancel: Option<&AtomicBool>,
    ) -> io::Result<Self> {
        sections::cancelled(cancel)?;
        let mut files = open_segment_files(path, max_segments.min(MAX_SEGMENTS))?;
        let inventory = sections::inventory(&mut files, cancel)?;
        let geometry = geometry(&mut files, &inventory)?;
        let total_bytes = geometry.total_bytes()?;
        let expected = total_bytes.div_ceil(geometry.chunk_bytes()?);
        let index = paged_index(&mut files, &inventory, expected, cancel)?;
        for (file, segment) in files.iter().zip(&inventory) {
            let metadata = file.metadata()?;
            if (metadata.len(), metadata.modified()?) != (segment.file_len, segment.modified) {
                return Err(sections::invalid("E01 segment changed while planning"));
            }
        }
        Ok(Self::from_parts(
            ReaderInfo {
                path: path.to_path_buf(),
                size: total_bytes,
                kind: "e01".into(),
            },
            total_bytes,
            geometry.sectors_per_chunk,
            geometry.bytes_per_sector,
            index,
            files,
        ))
    }
}

fn open_segment_files(path: &Path, max_segments: u32) -> io::Result<Vec<File>> {
    let mut files = Vec::new();
    for segment in 1..=max_segments {
        match File::open(build_segment_path(path, segment)) {
            Ok(file) => files.push(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    if build_segment_path(path, max_segments + 1).try_exists()? {
        return Err(crate::open_limits::limit());
    }
    if files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no E01 segments found",
        ));
    }
    Ok(files)
}

fn geometry(files: &mut [File], segments: &[SegmentSections]) -> io::Result<E01Geometry> {
    let mut views = Vec::new();
    let mut budget = OpenBudget::default();
    for (file, segment) in files.iter_mut().zip(segments) {
        for section in segment
            .sections
            .iter()
            .filter(|s| s.kind == "volume" || s.kind.starts_with("disk"))
        {
            let (_, size) = section.content_range(segment.file_len)?;
            budget.content(usize::try_from(size).map_err(|_| crate::open_limits::limit())?)?;
            views.push((
                section.kind.clone(),
                section.read_content(file, segment.file_len)?,
            ));
        }
    }
    find_geometry(&views)
}

fn paged_index(
    files: &mut [File],
    segments: &[SegmentSections],
    expected: u64,
    cancel: Option<&AtomicBool>,
) -> io::Result<PagedIndex> {
    let mut tables = Vec::new();
    let mut chunks = 0u64;
    let mut metadata = crate::paged_index::CACHE_BYTES;
    for (file, segment) in files.iter_mut().zip(segments) {
        let validated = crate::table_validation::validate_segment(file, segment, chunks, cancel)?;
        for table in &validated {
            chunks = chunks
                .checked_add(table.count)
                .ok_or_else(crate::open_limits::limit)?;
            metadata = metadata.saturating_add(
                std::mem::size_of_val(table) + table.page_checksums.capacity() * 32,
            );
        }
        if metadata > MAX_METADATA_BYTES {
            return Err(crate::open_limits::limit());
        }
        tables.extend(validated);
    }
    PagedIndex::new(tables, expected)
}
