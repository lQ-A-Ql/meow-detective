use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use evidence_block::BlockProvider;

use crate::format::{
    commit_digest, read_record, write_commit_record, write_data_record, write_superblock_slot,
    write_superblocks, DataPointer, ParsedRecord, PendingData, Superblock, DATA_START,
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
        write_superblocks(&mut file, &header)?;
        Ok(Self {
            file,
            header,
            index: HashMap::new(),
            poisoned: false,
            unsynced_bytes: 0,
        })
    }

    pub(crate) fn open(
        path: &Path,
        parent: &ParentIdentity,
        expected_cluster_size: u32,
    ) -> Result<Self, EmulationError> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let header = crate::format::read_superblock(&mut file)?;
        if &header.parent != parent {
            return Err(EmulationError::ParentMismatch);
        }
        if header.cluster_size != expected_cluster_size {
            return Err(EmulationError::CorruptOverlay(format!(
                "cluster size is {}, expected {expected_cluster_size}",
                header.cluster_size
            )));
        }
        let (index, committed_end) = recover_index(&mut file, &header)?;
        file.set_len(committed_end)?;
        file.sync_all()?;
        Ok(Self {
            file,
            header,
            index,
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
        let generation = self
            .header
            .generation
            .checked_add(1)
            .ok_or(EmulationError::ArithmeticOverflow)?;
        let mut pending = Vec::with_capacity(clusters.len());
        for (cluster_index, data) in clusters {
            if data.len() != self.header.cluster_size as usize {
                return Err(EmulationError::CorruptOverlay(
                    "transaction contains a partial cluster".to_string(),
                ));
            }
            pending.push(write_data_record(
                &mut self.file,
                generation,
                *cluster_index,
                data,
            )?);
        }
        write_commit_record(&mut self.file, generation, &pending)?;
        let mut next_header = self.header.clone();
        next_header.generation = generation;
        for item in &pending {
            self.index.insert(item.cluster_index, item.pointer);
        }
        self.header = next_header;
        self.unsynced_bytes = self.unsynced_bytes.saturating_add(
            pending
                .len()
                .saturating_mul(self.header.cluster_size as usize) as u64,
        );
        if self.unsynced_bytes >= SYNC_BATCH_BYTES {
            self.sync_pending()?;
        }
        Ok(())
    }

    fn sync_pending(&mut self) -> Result<(), EmulationError> {
        if self.unsynced_bytes == 0 {
            return Ok(());
        }
        // Publish the generation only after all data and commit records have
        // reached stable storage. Recovery can then safely truncate any
        // transaction whose generation is absent from the superblock.
        self.file.sync_all()?;
        write_superblock_slot(&mut self.file, &self.header)?;
        self.file.sync_all()?;
        self.unsynced_bytes = 0;
        Ok(())
    }

    fn ensure_usable(&self) -> Result<(), EmulationError> {
        if self.poisoned {
            return Err(EmulationError::CorruptOverlay(
                "overlay must be reopened after a failed write".to_string(),
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

fn recover_index(
    file: &mut File,
    header: &Superblock,
) -> Result<(HashMap<u64, DataPointer>, u64), EmulationError> {
    if header.generation == 0 {
        return Ok((HashMap::new(), DATA_START));
    }
    let file_length = file.metadata()?.len();
    let cluster_count = header
        .parent
        .logical_length()
        .div_ceil(u64::from(header.cluster_size));
    let mut index = HashMap::new();
    let mut pending = Vec::<PendingData>::new();
    let mut seen_clusters = HashSet::new();
    let mut expected_generation = 1u64;
    let mut offset = DATA_START;
    while let Some((record, next)) = read_record(file, offset, file_length, header.cluster_size)? {
        match record {
            ParsedRecord::Data(data) => {
                if data.generation != expected_generation || data.cluster_index >= cluster_count {
                    return Err(corrupt("data record is outside the expected transaction"));
                }
                if !seen_clusters.insert(data.cluster_index) {
                    return Err(corrupt("transaction contains duplicate cluster records"));
                }
                pending.push(data);
            }
            ParsedRecord::Commit {
                generation,
                count,
                digest,
            } => {
                if generation != expected_generation
                    || count != pending.len() as u64
                    || digest != commit_digest(generation, &pending)
                {
                    return Err(corrupt(
                        "transaction commit does not match its data records",
                    ));
                }
                for item in pending.drain(..) {
                    index.insert(item.cluster_index, item.pointer);
                }
                seen_clusters.clear();
                if generation == header.generation {
                    return Ok((index, next));
                }
                expected_generation = expected_generation
                    .checked_add(1)
                    .ok_or(EmulationError::ArithmeticOverflow)?;
            }
        }
        offset = next;
    }
    Err(corrupt(
        "overlay ended before the committed generation was recovered",
    ))
}

fn corrupt(message: &str) -> EmulationError {
    EmulationError::CorruptOverlay(message.to_string())
}
