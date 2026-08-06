use std::path::Path;
use std::sync::{Arc, Mutex};

use evidence_block::BlockProvider;

use crate::overlay::Overlay;
use crate::{EmulationError, ParentIdentity};

const DEFAULT_CLUSTER_SIZE: u32 = 64 * 1024;
const DEFAULT_MAX_WRITE: usize = 16 * 1024 * 1024;
const MIN_CLUSTER_SIZE: u32 = 4096;
const MAX_CLUSTER_SIZE: u32 = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct CowDiskConfig {
    pub cluster_size: u32,
    pub max_write_length: usize,
}

impl Default for CowDiskConfig {
    fn default() -> Self {
        Self {
            cluster_size: DEFAULT_CLUSTER_SIZE,
            max_write_length: DEFAULT_MAX_WRITE,
        }
    }
}

pub struct CowDisk {
    parent: Arc<dyn BlockProvider>,
    identity: ParentIdentity,
    config: CowDiskConfig,
    overlay: Mutex<Overlay>,
}

impl CowDisk {
    pub fn create(
        overlay_path: &Path,
        parent: Arc<dyn BlockProvider>,
        identity: ParentIdentity,
        config: CowDiskConfig,
    ) -> Result<Self, EmulationError> {
        validate_config(&identity, &parent, config)?;
        let overlay = Overlay::create(overlay_path, identity.clone(), config.cluster_size)?;
        Ok(Self {
            parent,
            identity,
            config,
            overlay: Mutex::new(overlay),
        })
    }

    pub fn open(
        overlay_path: &Path,
        parent: Arc<dyn BlockProvider>,
        identity: ParentIdentity,
        config: CowDiskConfig,
    ) -> Result<Self, EmulationError> {
        validate_config(&identity, &parent, config)?;
        let overlay = Overlay::open(overlay_path, &identity, config.cluster_size)?;
        Ok(Self {
            parent,
            identity,
            config,
            overlay: Mutex::new(overlay),
        })
    }

    pub fn len(&self) -> u64 {
        self.identity.logical_length()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), EmulationError> {
        validate_range(offset, buffer.len(), self.len())?;
        let mut overlay = self
            .overlay
            .lock()
            .map_err(|_| EmulationError::LockPoisoned)?;
        let cluster_size = self.config.cluster_size as usize;
        let mut cluster = vec![0u8; cluster_size];
        let mut copied = 0usize;
        while copied < buffer.len() {
            let position = offset + copied as u64;
            let cluster_index = position / self.config.cluster_size as u64;
            let intra = (position % self.config.cluster_size as u64) as usize;
            overlay.read_cluster(&self.parent, cluster_index, &mut cluster)?;
            let count = (cluster_size - intra).min(buffer.len() - copied);
            buffer[copied..copied + count].copy_from_slice(&cluster[intra..intra + count]);
            copied += count;
        }
        Ok(())
    }

    pub fn write_all_at(&self, offset: u64, buffer: &[u8]) -> Result<(), EmulationError> {
        if buffer.len() > self.config.max_write_length {
            return Err(EmulationError::WriteTooLarge {
                requested: buffer.len(),
                maximum: self.config.max_write_length,
            });
        }
        validate_range(offset, buffer.len(), self.len())?;
        if buffer.is_empty() {
            return Ok(());
        }
        let mut overlay = self
            .overlay
            .lock()
            .map_err(|_| EmulationError::LockPoisoned)?;
        let cluster_size = self.config.cluster_size as usize;
        let first = offset / cluster_size as u64;
        let last = (offset + buffer.len() as u64 - 1) / cluster_size as u64;
        let mut clusters = Vec::with_capacity((last - first + 1) as usize);
        let mut consumed = 0usize;
        for cluster_index in first..=last {
            let mut cluster = vec![0u8; cluster_size];
            overlay.read_cluster(&self.parent, cluster_index, &mut cluster)?;
            let cluster_start = cluster_index * cluster_size as u64;
            let write_start = offset.max(cluster_start);
            let write_end = (offset + buffer.len() as u64).min(cluster_start + cluster_size as u64);
            let intra = (write_start - cluster_start) as usize;
            let count = (write_end - write_start) as usize;
            cluster[intra..intra + count].copy_from_slice(&buffer[consumed..consumed + count]);
            consumed += count;
            clusters.push((cluster_index, cluster));
        }
        overlay.commit_clusters(&clusters)
    }

    pub fn flush(&self) -> Result<(), EmulationError> {
        self.overlay
            .lock()
            .map_err(|_| EmulationError::LockPoisoned)?
            .flush()
    }
}

fn validate_config(
    identity: &ParentIdentity,
    parent: &Arc<dyn BlockProvider>,
    config: CowDiskConfig,
) -> Result<(), EmulationError> {
    if config.cluster_size < MIN_CLUSTER_SIZE
        || config.cluster_size > MAX_CLUSTER_SIZE
        || !config.cluster_size.is_power_of_two()
    {
        return Err(EmulationError::InvalidClusterSize(config.cluster_size));
    }
    if parent.len() != identity.logical_length() {
        return Err(EmulationError::ParentMismatch);
    }
    Ok(())
}

fn validate_range(offset: u64, length: usize, disk_length: u64) -> Result<(), EmulationError> {
    let end = offset
        .checked_add(length as u64)
        .ok_or(EmulationError::ArithmeticOverflow)?;
    if end > disk_length {
        return Err(EmulationError::OutOfBounds {
            offset,
            end,
            length: disk_length,
        });
    }
    Ok(())
}
