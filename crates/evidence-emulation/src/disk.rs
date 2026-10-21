use std::path::Path;
use std::sync::{Arc, Mutex};

use evidence_block::BlockProvider;

use crate::cache::ClusterLru;
use crate::overlay::Overlay;
use crate::{EmulationError, ParentIdentity};

const DEFAULT_CLUSTER_SIZE: u32 = 64 * 1024;
const DEFAULT_MAX_WRITE: usize = 16 * 1024 * 1024;
const MIN_CLUSTER_SIZE: u32 = 4096;
const MAX_CLUSTER_SIZE: u32 = 1024 * 1024;
// VMware probes the flat extent with adjacent small reads during power-on.
// Keep the window bounded so this reduces E01/Dokan crossings without making
// random guest access scale with the evidence size.
const READ_AHEAD_BYTES: usize = 256 * 1024;
const MAX_READ_AHEAD_CLUSTERS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct CowDiskConfig {
    pub cluster_size: u32,
    pub max_write_length: usize,
    pub source_volume: Option<crate::SourceVolumeMapping>,
}

impl Default for CowDiskConfig {
    fn default() -> Self {
        Self {
            cluster_size: DEFAULT_CLUSTER_SIZE,
            max_write_length: DEFAULT_MAX_WRITE,
            source_volume: None,
        }
    }
}

pub struct CowDisk {
    parent: Arc<dyn BlockProvider>,
    identity: ParentIdentity,
    config: CowDiskConfig,
    state: Mutex<DiskState>,
}

struct DiskState {
    overlay: Overlay,
    cache: ClusterLru,
}

impl CowDisk {
    pub fn source_volume_mapping(&self) -> Option<crate::SourceVolumeMapping> {
        self.config.source_volume
    }
    pub fn evidence_partition_offset(&self, offset: u64) -> Result<u64, EmulationError> {
        self.config
            .source_volume
            .map_or(Ok(offset), |mapping| mapping.partition_offset(offset))
    }
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
            state: Mutex::new(DiskState {
                overlay,
                cache: ClusterLru::new(config.cluster_size),
            }),
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
        let cluster_size = self.config.cluster_size as usize;
        let mut cluster = vec![0u8; cluster_size];
        let mut copied = 0usize;
        while copied < buffer.len() {
            let position = offset + copied as u64;
            let cluster_index = position / self.config.cluster_size as u64;
            let intra = (position % self.config.cluster_size as u64) as usize;
            let mut state = self
                .state
                .lock()
                .map_err(|_| EmulationError::LockPoisoned)?;
            state.overlay.ensure_readable()?;
            let cached = state.cache.copy_to(cluster_index, &mut cluster);
            let overlay_hit = !cached
                && state
                    .overlay
                    .read_overlay_cluster(cluster_index, &mut cluster)?;
            if !cached && !overlay_hit {
                // Parent I/O and E01 decompression must not block writers or
                // unrelated VM reads that only need the overlay state.
                drop(state);
                let read_ahead_clusters = read_ahead_clusters(cluster_size);
                let mut prefetched = vec![0u8; cluster_size * read_ahead_clusters];
                read_parent_window(
                    &self.parent,
                    self.len(),
                    self.config.cluster_size,
                    cluster_index,
                    &mut prefetched,
                )?;
                state = self
                    .state
                    .lock()
                    .map_err(|_| EmulationError::LockPoisoned)?;
                state.overlay.ensure_readable()?;
                for (index, data) in prefetched.chunks_exact(cluster_size).enumerate() {
                    let index = cluster_index.saturating_add(index as u64);
                    if !state.cache.contains(index) && !state.overlay.contains_cluster(index) {
                        state.cache.insert(index, data);
                    }
                }
                // A concurrent write may have committed while the parent was
                // being read. Prefer that newer overlay data over stale cache
                // contents, then use the authoritative view for this request.
                if !state.cache.copy_to(cluster_index, &mut cluster) {
                    state
                        .overlay
                        .read_overlay_cluster(cluster_index, &mut cluster)?;
                }
            } else if overlay_hit {
                state.cache.insert(cluster_index, &cluster);
            }
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
        let mut state = self
            .state
            .lock()
            .map_err(|_| EmulationError::LockPoisoned)?;
        let cluster_size = self.config.cluster_size as usize;
        let first = offset / cluster_size as u64;
        let last = (offset + buffer.len() as u64 - 1) / cluster_size as u64;
        let mut clusters = Vec::with_capacity((last - first + 1) as usize);
        let mut consumed = 0usize;
        for cluster_index in first..=last {
            let cluster_start = cluster_index * cluster_size as u64;
            let write_start = offset.max(cluster_start);
            let write_end = (offset + buffer.len() as u64).min(cluster_start + cluster_size as u64);
            let intra = (write_start - cluster_start) as usize;
            let count = (write_end - write_start) as usize;
            let mut cluster = vec![0u8; cluster_size];
            // A write covering the whole cluster makes the previous contents
            // irrelevant, so skip the overlay/parent read-through entirely.
            if count != cluster_size && !state.cache.copy_to(cluster_index, &mut cluster) {
                state
                    .overlay
                    .read_cluster(&self.parent, cluster_index, &mut cluster)?;
                state.cache.insert(cluster_index, &cluster);
            }
            cluster[intra..intra + count].copy_from_slice(&buffer[consumed..consumed + count]);
            consumed += count;
            clusters.push((cluster_index, cluster));
        }
        state.overlay.commit_clusters(&clusters)?;
        for (cluster_index, data) in clusters {
            state.cache.insert(cluster_index, &data);
        }
        Ok(())
    }

    pub fn flush(&self) -> Result<(), EmulationError> {
        self.state
            .lock()
            .map_err(|_| EmulationError::LockPoisoned)?
            .overlay
            .flush()
    }

    /// True after a failed write poisoned the overlay. A poisoned overlay
    /// cannot be flushed or extended; the session must be released.
    pub fn is_poisoned(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.overlay.is_poisoned())
            .unwrap_or(true)
    }

    /// Permanently invalidate this disposable session after a higher-level
    /// multi-write operation can no longer prove the overlay consistent.
    pub fn invalidate(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.overlay.poison();
            state.cache.clear();
        }
    }
}

fn read_parent_window(
    parent: &Arc<dyn BlockProvider>,
    disk_length: u64,
    cluster_size: u32,
    cluster_index: u64,
    buffer: &mut [u8],
) -> Result<(), EmulationError> {
    buffer.fill(0);
    let offset = cluster_index
        .checked_mul(u64::from(cluster_size))
        .ok_or(EmulationError::ArithmeticOverflow)?;
    let available = disk_length.saturating_sub(offset).min(buffer.len() as u64) as usize;
    if available != 0 {
        parent.read_exact_at(offset, &mut buffer[..available])?;
    }
    Ok(())
}

fn read_ahead_clusters(cluster_size: usize) -> usize {
    (READ_AHEAD_BYTES / cluster_size.max(1)).clamp(1, MAX_READ_AHEAD_CLUSTERS)
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
    // Length is the only parent-identity half this crate can enforce: the
    // recorded SHA-256 covers the evidence container bytes (E01 segment
    // files), which are not observable through `BlockProvider`. The
    // session-preparation path re-hashes the container against the persisted
    // import fingerprint and fails closed before `CowDisk::create`.
    if parent.len() != identity.logical_length() {
        return Err(EmulationError::ParentMismatch);
    }
    if let Some(mapping) = config.source_volume {
        if mapping.guest_length()? != parent.len()
            || mapping.source_volume_length == 0
            || !mapping.source_volume_length.is_multiple_of(512)
            || mapping.disk_signature == 0
            || mapping.source_volume_length > mapping.source_media_length
            || mapping.guest_volume_offset != crate::NTFS_GUEST_VOLUME_OFFSET
        {
            return Err(EmulationError::ParentMismatch);
        }
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
