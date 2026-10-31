//! An immutable, handle-free open plan shared by independent E01 readers.
use crate::{build_segment_path, E01Reader};
use evidence_core::ReaderInfo;
use std::{io, sync::Arc, time::SystemTime};

#[derive(Clone)]
pub struct E01OpenPlan {
    info: ReaderInfo,
    total_bytes: u64,
    chunk_size_sectors: u32,
    bytes_per_sector: u32,
    chunk_table: Arc<crate::paged_index::PagedIndex>,
    max_stored_chunk: u64,
    segments: Vec<(u64, SystemTime)>,
    resource_metadata: Option<Arc<dyn evidence_core::reader::ReaderResourceHold>>,
    resource_admission: Option<Arc<dyn evidence_core::reader::ReaderAdmission>>,
}

impl E01Reader {
    pub fn open_plan(self) -> io::Result<E01OpenPlan> {
        let segments = self
            .segment_files
            .iter()
            .map(|file| {
                let metadata = file.metadata()?;
                Ok((metadata.len(), metadata.modified()?))
            })
            .collect::<io::Result<_>>()?;
        let max_stored_chunk = self.chunk_table.max_stored_chunk();
        Ok(E01OpenPlan {
            info: self.info,
            total_bytes: self.total_bytes,
            chunk_size_sectors: self.chunk_size_sectors,
            bytes_per_sector: self.bytes_per_sector,
            chunk_table: self.chunk_table,
            max_stored_chunk,
            segments,
            resource_metadata: self.resource_metadata,
            resource_admission: self.resource_admission,
        })
    }
}

impl E01OpenPlan {
    pub fn segment_paths(&self) -> Vec<std::path::PathBuf> {
        (1..=self.segments.len())
            .map(|index| build_segment_path(&self.info.path, index as u32))
            .collect()
    }
    /// Metadata identity only; does not claim to hash or freeze the evidence.
    pub fn validate_snapshot(&self) -> io::Result<()> {
        for (index, expected) in self.segments.iter().enumerate() {
            let metadata =
                std::fs::metadata(build_segment_path(&self.info.path, index as u32 + 1))?;
            if (metadata.len(), metadata.modified()?) != *expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "E01 segment changed since planning",
                ));
            }
        }
        if build_segment_path(&self.info.path, self.segments.len() as u32 + 1).try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "E01 segment set changed since planning",
            ));
        }
        Ok(())
    }
    pub fn open_reader(&self) -> io::Result<E01Reader> {
        let resource_reader = self
            .resource_admission
            .as_ref()
            .map(|a| a.acquire())
            .transpose()?;
        let mut files = Vec::with_capacity(self.segments.len());
        for (index, expected) in self.segments.iter().enumerate() {
            let file = std::fs::File::open(build_segment_path(&self.info.path, index as u32 + 1))?;
            let metadata = file.metadata()?;
            if (metadata.len(), metadata.modified()?) != *expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "E01 segment changed since planning",
                ));
            }
            files.push(file);
        }
        if build_segment_path(&self.info.path, files.len() as u32 + 1).try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "E01 segment set changed since planning",
            ));
        }
        Ok(E01Reader {
            info: self.info.clone(),
            total_bytes: self.total_bytes,
            chunk_size_sectors: self.chunk_size_sectors,
            bytes_per_sector: self.bytes_per_sector,
            chunk_table: Arc::clone(&self.chunk_table),
            segment_files: files,
            segment_metadata: Some(self.segments.clone()),
            cursor: 0,
            chunk_cache: Default::default(),
            chunk_cache_bytes: 0,
            chunk_cache_enabled: true,
            last_chunk_read: None,
            resource_metadata: self.resource_metadata.clone(),
            resource_admission: self.resource_admission.clone(),
            _resource_reader: resource_reader,
        })
    }
    pub fn metadata_bytes(&self) -> u64 {
        (self.chunk_table.metadata_bytes()
            + self.segments.capacity() * std::mem::size_of::<(u64, SystemTime)>()
            + self.info.path.capacity()
            + self.segments.len() * (2 * self.info.path.capacity() + 128)
            + 65536
            + std::mem::size_of::<Self>()) as u64
    }
    pub fn reader_memory_bound(&self) -> u64 {
        let chunk = u64::from(self.chunk_size_sectors) * u64::from(self.bytes_per_sector);
        let stored = self.max_stored_chunk;
        chunk
            .saturating_mul(4)
            .saturating_add(stored.saturating_mul(2))
            .saturating_add(chunk.max(16 * 1024 * 1024))
            .saturating_add(
                65536
                    + 2 * crate::paged_index::PAGE_BYTES as u64
                    + 4 * self.info.path.capacity() as u64
                    + 64 * self.segments.len() as u64,
            )
    }
    pub fn with_reader_admission(
        mut self,
        admission: Arc<dyn evidence_core::reader::ReaderAdmission>,
    ) -> Self {
        self.resource_admission = Some(admission);
        self
    }
    pub fn with_resources(
        mut self,
        metadata: Arc<dyn evidence_core::reader::ReaderResourceHold>,
        admission: Arc<dyn evidence_core::reader::ReaderAdmission>,
    ) -> Self {
        self.resource_metadata = Some(metadata);
        self.resource_admission = Some(admission);
        self
    }
    pub fn logical_size(&self) -> u64 {
        self.total_bytes
    }
    pub fn chunk_count(&self) -> usize {
        self.chunk_table.len()
    }
    /// Number of validated table2 sections used by the bounded paged plan.
    pub fn mirrored_table_count(&self) -> usize {
        self.chunk_table.mirrored_tables()
    }
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}
