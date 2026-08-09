use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use evidence_block::BlockProvider;

use crate::format::{
    aligned_record_length, write_data_record, write_superblocks, DataPointer, Superblock,
};
use crate::{EmulationError, ParentIdentity};

pub(crate) struct Overlay {
    file: File,
    header: Superblock,
    index: HashMap<u64, DataPointer>,
    poisoned: bool,
    unsynced_bytes: u64,
}

const SYNC_BATCH_BYTES: u64 = 4 * 1024 * 1024;

impl Overlay {
    pub(crate) fn create(
        path: &Path,
        parent: ParentIdentity,
        cluster_size: u32,
    ) -> Result<Self, EmulationError> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    EmulationError::OverlayExists(path.to_path_buf())
                } else {
                    EmulationError::Io(error)
                }
            })?;
        let header = Superblock::new(parent, cluster_size);
        if let Err(error) = write_superblocks(&mut file, &header) {
            // Do not strand a half-written overlay: it would block every
            // later create on the same path with `OverlayExists`.
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(Self {
            file,
            header,
            index: HashMap::new(),
            poisoned: false,
            unsynced_bytes: 0,
        })
    }

    pub(crate) fn read_cluster(
        &mut self,
        parent: &Arc<dyn BlockProvider>,
        cluster_index: u64,
        buffer: &mut [u8],
    ) -> Result<(), EmulationError> {
        self.ensure_usable()?;
        if let Some(pointer) = self.index.get(&cluster_index) {
            self.file.seek(SeekFrom::Start(pointer.data_offset))?;
            self.file.read_exact(buffer)?;
            return Ok(());
        }
        buffer.fill(0);
        let offset = cluster_index
            .checked_mul(u64::from(self.header.cluster_size))
            .ok_or(EmulationError::ArithmeticOverflow)?;
        let available = self
            .header
            .parent
            .logical_length()
            .saturating_sub(offset)
            .min(buffer.len() as u64) as usize;
        if available != 0 {
            parent.read_exact_at(offset, &mut buffer[..available])?;
        }
        Ok(())
    }

    pub(crate) fn commit_clusters(
        &mut self,
        clusters: &[(u64, Vec<u8>)],
    ) -> Result<(), EmulationError> {
        self.ensure_usable()?;
        let result = self.commit_clusters_inner(clusters);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    pub(crate) fn flush(&mut self) -> Result<(), EmulationError> {
        self.ensure_usable()?;
        self.sync_pending()
    }

    fn commit_clusters_inner(&mut self, clusters: &[(u64, Vec<u8>)]) -> Result<(), EmulationError> {
        let mut pointers = Vec::with_capacity(clusters.len());
        for (cluster_index, data) in clusters {
            if data.len() != self.header.cluster_size as usize {
                return Err(EmulationError::CorruptOverlay(
                    "transaction contains a partial cluster".to_string(),
                ));
            }
            pointers.push((
                *cluster_index,
                write_data_record(&mut self.file, *cluster_index, data)?,
            ));
        }
        // The in-memory index is updated only after every record reached the
        // file, so a failed transaction leaves unreachable bytes behind but
        // never pollutes the logical view.
        for (cluster_index, pointer) in pointers {
            self.index.insert(cluster_index, pointer);
        }
        // Account for the real on-disk footprint of each record (header plus
        // alignment padding), not just the cluster payload, so the sync
        // threshold reflects the actual unsynced byte volume.
        self.unsynced_bytes = clusters
            .iter()
            .fold(self.unsynced_bytes, |total, (_, data)| {
                total.saturating_add(aligned_record_length(data.len()))
            });
        if self.unsynced_bytes >= SYNC_BATCH_BYTES {
            self.sync_pending()?;
        }
        Ok(())
    }

    fn sync_pending(&mut self) -> Result<(), EmulationError> {
        if self.unsynced_bytes == 0 {
            return Ok(());
        }
        self.file.sync_all()?;
        self.unsynced_bytes = 0;
        Ok(())
    }

    fn ensure_usable(&self) -> Result<(), EmulationError> {
        if self.poisoned {
            return Err(EmulationError::CorruptOverlay(
                "overlay session must be released and recreated after a failed write".to_string(),
            ));
        }
        Ok(())
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = self.sync_pending();
    }
}
