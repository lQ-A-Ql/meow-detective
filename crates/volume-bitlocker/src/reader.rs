//! The plaintext `Read + Seek` view of an unlocked volume.
//!
//! Derived from `bitlocker-core`'s `DecryptedVolume` (see `../NOTICE`), with the
//! sharing and caching shape this project's read path needs.
//!
//! ```text
//! E01 / RAW -> PartitionWindowReader -> BitLockerReader -> NTFS / FAT / exFAT
//! ```
//!
//! # Sharing shape
//!
//! [`UnlockedVolume`] holds the cipher and the layout and is immutable once
//! built, so many readers share one `Arc` of it. Each [`BitLockerReader`] owns
//! only its own evidence handle, position, and sector cache. That is what makes a
//! per-read reader cheap: the key derivation ran once at unlock and is never
//! repeated, no matter how many times the same range is read.
//!
//! The original evidence handle is only ever read from. Nothing here writes to
//! the image, mounts anything, or materializes a plaintext copy.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use crate::cipher::{SectorCipher, CIPHER_SECTOR_SIZE};
use crate::error::{BitLockerError, Result};
use crate::layout::{sector_start, SectorSource, VolumeLayout};
use crate::metadata::FveMetadata;
use crate::secret::VolumeKeyPackage;

/// Cached sector count. 256 x 512 B = 128 KiB per reader.
///
/// The cache is direct-mapped, so it is bounded by construction rather than by an
/// eviction policy that could be wrong: a sector can only ever occupy one slot,
/// and the memory cost is fixed the moment the reader is built.
const CACHE_SLOTS: usize = 256;

/// Largest single read issued against the evidence handle, in bytes.
///
/// Sequential reads coalesce up to this much ciphertext per seek instead of one
/// 512-byte read per sector. It also caps the transient buffer and each `Read`
/// call, so a caller passing an enormous buffer cannot pull an unbounded span
/// through the cipher at once.
const MAX_COALESCED_READ: usize = 1024 * 1024;

/// The immutable, shared result of unlocking a volume.
///
/// Not `Debug`, `Clone`, or `Serialize`: it owns a [`SectorCipher`], which holds
/// expanded key schedules.
pub struct UnlockedVolume {
    cipher: SectorCipher,
    layout: VolumeLayout,
}

impl UnlockedVolume {
    /// Builds the shared unlock state from metadata and a verified key package.
    ///
    /// # Errors
    ///
    /// Whatever [`SectorCipher::new`] rejects: an unsupported method, or a key
    /// package whose length does not match the method.
    pub fn new(metadata: &FveMetadata, keys: &VolumeKeyPackage) -> Result<Self> {
        Ok(Self {
            cipher: SectorCipher::new(metadata.encryption_method, keys)?,
            layout: VolumeLayout::from_metadata(metadata),
        })
    }

    /// The volume's address-space layout.
    #[must_use]
    pub fn layout(&self) -> &VolumeLayout {
        &self.layout
    }
}

/// One cache slot.
struct CachedSector {
    /// Logical sector start this slot holds, or `None` when empty.
    ///
    /// Stored rather than derived so a slot collision is a miss, not a silent
    /// wrong hit.
    tag: Option<u64>,
    bytes: [u8; CIPHER_SECTOR_SIZE],
}

/// A fixed-size, direct-mapped plaintext sector cache.
struct SectorCache {
    slots: Vec<CachedSector>,
}

impl SectorCache {
    fn new() -> Self {
        Self {
            slots: (0..CACHE_SLOTS)
                .map(|_| CachedSector {
                    tag: None,
                    bytes: [0u8; CIPHER_SECTOR_SIZE],
                })
                .collect(),
        }
    }

    /// The slot a logical sector maps to.
    fn slot_index(logical_start: u64) -> usize {
        ((logical_start / CIPHER_SECTOR_SIZE as u64) % CACHE_SLOTS as u64) as usize
    }

    fn get(&self, logical_start: u64) -> Option<&[u8; CIPHER_SECTOR_SIZE]> {
        let slot = &self.slots[Self::slot_index(logical_start)];
        (slot.tag == Some(logical_start)).then_some(&slot.bytes)
    }

    fn put(&mut self, logical_start: u64, bytes: &[u8; CIPHER_SECTOR_SIZE]) {
        let slot = &mut self.slots[Self::slot_index(logical_start)];
        slot.tag = Some(logical_start);
        slot.bytes = *bytes;
    }
}

/// A plaintext view over one unlocked BitLocker volume.
///
/// Not `Clone`: each reader owns its own evidence handle and stream position.
/// Callers wanting a second concurrent view build another reader from the same
/// [`Arc<UnlockedVolume>`].
pub struct BitLockerReader<R> {
    volume: Arc<UnlockedVolume>,
    inner: R,
    /// Total plaintext length presented, in bytes.
    length: u64,
    position: u64,
    cache: SectorCache,
}

impl<R: Read + Seek> BitLockerReader<R> {
    /// Wraps a partition-window reader in the plaintext view.
    ///
    /// `inner` must present the encrypted volume's byte 0 at its own offset 0.
    ///
    /// # Errors
    ///
    /// [`BitLockerError::EvidenceRead`] when the volume length cannot be
    /// determined.
    pub fn new(volume: Arc<UnlockedVolume>, mut inner: R) -> Result<Self> {
        let length = inner
            .seek(SeekFrom::End(0))
            .map_err(|source| BitLockerError::EvidenceRead { offset: 0, source })?;
        inner
            .seek(SeekFrom::Start(0))
            .map_err(|source| BitLockerError::EvidenceRead { offset: 0, source })?;
        Ok(Self {
            volume,
            inner,
            length,
            position: 0,
            cache: SectorCache::new(),
        })
    }

    /// The plaintext length of the volume.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.length
    }

    /// Whether the volume presents no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Reads plaintext at `offset` into `buf`, filling it completely.
    ///
    /// Bytes past the end of the volume read back as zero, matching how the
    /// underlying evidence readers behave at EOF.
    ///
    /// # Errors
    ///
    /// [`BitLockerError::EvidenceRead`] when the underlying handle fails, and
    /// [`BitLockerError::OutOfBounds`] when the requested span cannot be
    /// addressed.
    pub fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<()> {
        // Reject an offset+length that cannot be addressed at all, rather than
        // wrapping into a valid-looking span.
        offset
            .checked_add(buf.len() as u64)
            .ok_or(BitLockerError::OutOfBounds {
                offset,
                length: buf.len() as u64,
                volume_length: self.length,
            })?;

        let mut done = 0usize;
        while done < buf.len() {
            let position = offset + done as u64;
            let start = sector_start(position);
            let within = (position - start) as usize;
            let take = (CIPHER_SECTOR_SIZE - within).min(buf.len() - done);
            if self.cache.get(start).is_none() {
                // Warm the cache for this sector and as much of the following run
                // as one bounded read can cover.
                self.load_run(start, buf.len() - done)?;
            }
            let sector = self.cache.get(start).ok_or(BitLockerError::OutOfBounds {
                offset: start,
                length: CIPHER_SECTOR_SIZE as u64,
                volume_length: self.length,
            })?;
            buf[done..done + take].copy_from_slice(&sector[within..within + take]);
            done += take;
        }
        Ok(())
    }

    /// Loads a run of consecutive logical sectors starting at `start`.
    ///
    /// Sequential reads would otherwise issue one seek plus one 512-byte read per
    /// sector — 2048 pairs per megabyte. This coalesces a physically contiguous,
    /// uniformly-classified run into a single bounded read.
    ///
    /// The run stops at the first sector whose source disagrees with the run's, so
    /// a metadata block or the relocated-header boundary never gets folded into a
    /// neighbouring read.
    fn load_run(&mut self, start: u64, wanted: usize) -> Result<()> {
        let first = self.volume.layout.resolve(start);
        let (base, encrypted) = match first {
            SectorSource::Blanked => {
                self.cache.put(start, &[0u8; CIPHER_SECTOR_SIZE]);
                return Ok(());
            }
            SectorSource::Plaintext { physical_offset } => (physical_offset, false),
            SectorSource::Encrypted { physical_offset } => (physical_offset, true),
        };

        let wanted_sectors = wanted.div_ceil(CIPHER_SECTOR_SIZE).max(1);
        let cap = (MAX_COALESCED_READ / CIPHER_SECTOR_SIZE).min(CACHE_SLOTS);
        let mut run = 1usize;
        while run < wanted_sectors.min(cap) {
            let next_logical = start + (run * CIPHER_SECTOR_SIZE) as u64;
            let expected_physical = base + (run * CIPHER_SECTOR_SIZE) as u64;
            let matches = match self.volume.layout.resolve(next_logical) {
                SectorSource::Encrypted { physical_offset } => {
                    encrypted && physical_offset == expected_physical
                }
                SectorSource::Plaintext { physical_offset } => {
                    !encrypted && physical_offset == expected_physical
                }
                SectorSource::Blanked => false,
            };
            if !matches {
                break;
            }
            run += 1;
        }

        let mut buffer = vec![0u8; run * CIPHER_SECTOR_SIZE];
        let present = self.read_physical_span(base, &mut buffer)?;
        for index in 0..run {
            let span = index * CIPHER_SECTOR_SIZE..(index + 1) * CIPHER_SECTOR_SIZE;
            let mut sector = [0u8; CIPHER_SECTOR_SIZE];
            sector.copy_from_slice(&buffer[span.clone()]);
            // A sector with no bytes on the image does not exist. Running its
            // zero-filled buffer through the cipher would return 512 bytes of
            // plausible-looking garbage instead of the absence of data — the worst
            // possible answer on an evidence path, because nothing reports it.
            if encrypted && span.start < present {
                let physical = base + (index * CIPHER_SECTOR_SIZE) as u64;
                self.volume.cipher.decrypt_sector(&mut sector, physical);
            }
            self.cache
                .put(start + (index * CIPHER_SECTOR_SIZE) as u64, &sector);
        }
        Ok(())
    }

    /// Reads a raw span, zero-filling past EOF, and reports how many bytes were
    /// actually present on the image.
    ///
    /// A short read at the end is not an error: the evidence readers present a
    /// bounded image, and a filesystem can address past it. The count is what lets
    /// the caller tell an absent sector from a ciphertext one.
    fn read_physical_span(&mut self, physical_offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.inner
            .seek(SeekFrom::Start(physical_offset))
            .map_err(|source| BitLockerError::EvidenceRead {
                offset: physical_offset,
                source,
            })?;
        let mut filled = 0usize;
        while filled < buf.len() {
            match self.inner.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(source) => {
                    return Err(BitLockerError::EvidenceRead {
                        offset: physical_offset,
                        source,
                    })
                }
            }
        }
        // The tail stays zero: `buf` arrives zeroed and only the first `filled`
        // bytes are overwritten.
        Ok(filled)
    }
}

impl<R: Read + Seek> Read for BitLockerReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.length || buf.is_empty() {
            return Ok(0);
        }
        let remaining = self.length - self.position;
        // Cap each call so a caller passing an enormous buffer cannot make one
        // read pull an unbounded span through the cipher.
        let take = (buf.len() as u64)
            .min(remaining)
            .min(MAX_COALESCED_READ as u64) as usize;
        self.read_at(self.position, &mut buf[..take])
            .map_err(std::io::Error::other)?;
        self.position += take as u64;
        Ok(take)
    }
}

impl<R: Read + Seek> Seek for BitLockerReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if target < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start of the volume",
            ));
        }
        // Seeking past the end is legal and reads there return zero bytes, which
        // is the same contract `evidence_core::RawImageReader` presents.
        self.position = target as u64;
        Ok(self.position)
    }
}

#[cfg(test)]
#[path = "../tests/unit/reader/mod.rs"]
mod tests;
