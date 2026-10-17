//! An immutable, handle-free open plan shared by independent E01 readers.
use crate::{build_segment_path, E01Reader};
use evidence_core::ReaderInfo;
use std::{io, sync::Arc, time::SystemTime};

pub struct E01OpenPlan {
    info: ReaderInfo,
    total_bytes: u64,
    chunk_size_sectors: u32,
    bytes_per_sector: u32,
    chunk_table: Arc<Vec<(usize, u64, bool, u64)>>,
    segments: Vec<(u64, SystemTime)>,
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
        Ok(E01OpenPlan {
            info: self.info,
            total_bytes: self.total_bytes,
            chunk_size_sectors: self.chunk_size_sectors,
            bytes_per_sector: self.bytes_per_sector,
            chunk_table: self.chunk_table,
            segments,
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
        if build_segment_path(&self.info.path, files.len() as u32 + 1).exists() {
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
            cursor: 0,
            chunk_cache: Default::default(),
            chunk_cache_bytes: 0,
            last_chunk_read: None,
        })
    }
    pub fn logical_size(&self) -> u64 {
        self.total_bytes
    }
    pub fn chunk_count(&self) -> usize {
        self.chunk_table.len()
    }
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}
