//! Synthetic ciphertext images for the reader tests.
//!
//! The reader's job is address mapping, caching, and stream semantics — not
//! parsing. So these fixtures construct [`FveMetadata`] directly and lay
//! ciphertext into a buffer, rather than assembling a whole parseable volume. An
//! end-to-end pass over a real parsed volume is the public oracle's job.
//!
//! Method `0x8002` (AES-128-CBC, no diffuser) is used throughout: it is the
//! cheapest transform that still depends on the byte offset, which is the property
//! every mapping test turns on. Method coverage lives in the cipher tests.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use aes::cipher::block_padding::NoPadding;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit};
use aes::Aes128;

use crate::metadata::FveMetadata;
use crate::method::EncryptionMethod;
use crate::reader::{BitLockerReader, UnlockedVolume};
use crate::secret::VolumeKeyPackage;
use crate::Result;

/// Total length of the synthetic images.
pub(crate) const IMAGE_LEN: u64 = 0x60000;
/// Bytes of the volume start that are relocated.
pub(crate) const HEADER_SIZE: u64 = 0x2000;
/// Where the relocated volume header is stored.
pub(crate) const RELOCATED_OFFSET: u64 = 0x50000;
/// Where the single FVE metadata block sits.
pub(crate) const META_OFFSET: u64 = 0x4000;
/// Size of the metadata block region.
pub(crate) const META_SIZE: u64 = 0x800;

/// The 16-byte FVEK every fixture uses.
const FVEK: [u8; 16] = [
    0x11, 0x10, 0x13, 0x12, 0x15, 0x14, 0x17, 0x16, 0x19, 0x18, 0x1b, 0x1a, 0x1d, 0x1c, 0x1f, 0x1e,
];

/// The plaintext a sector at `physical_offset` is built to hold.
///
/// Derived from the offset so a misdirected read is visibly wrong rather than
/// coincidentally equal.
pub(crate) fn plaintext_pattern(physical_offset: u64) -> [u8; 512] {
    let mut sector = [0u8; 512];
    let seed = physical_offset.wrapping_mul(0x9E37_79B9);
    for (index, byte) in sector.iter_mut().enumerate() {
        *byte = ((seed >> (index % 8 * 8)) as u8) ^ (index as u8);
    }
    sector
}

/// AES-128-CBC-encrypts one sector in place with the BitLocker-derived IV.
fn cbc128_encrypt(sector: &mut [u8], offset: u64) {
    let mut iv_block = [0u8; 16];
    iv_block[0..8].copy_from_slice(&offset.to_le_bytes());
    let mut iv = GenericArray::clone_from_slice(&iv_block);
    Aes128::new(GenericArray::from_slice(&FVEK)).encrypt_block(&mut iv);

    let len = sector.len() - (sector.len() % 16);
    cbc::Encryptor::<Aes128>::new(GenericArray::from_slice(&FVEK), &iv)
        .encrypt_padded_mut::<NoPadding>(&mut sector[..len], len)
        .expect("NoPadding CBC over a 16-byte multiple cannot fail");
}

/// I/O counters shared with the cursor.
///
/// Held outside the reader so a test can inspect them without the reader
/// exposing its inner handle — a production accessor added purely for test
/// convenience would be dead code in a shipped build.
#[derive(Default)]
pub(crate) struct Counters {
    reads: std::sync::atomic::AtomicUsize,
    seeks: std::sync::atomic::AtomicUsize,
}

impl Counters {
    pub(crate) fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn seeks(&self) -> usize {
        self.seeks.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn bump(counter: &std::sync::atomic::AtomicUsize) {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A `Read + Seek` cursor that counts the operations issued against it.
///
/// Used to prove the cache and the run coalescing actually reduce evidence I/O
/// rather than asserting they exist and hoping.
pub(crate) struct CountingCursor {
    bytes: Vec<u8>,
    position: u64,
    counters: Arc<Counters>,
}

impl CountingCursor {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self::with_counters(bytes, Arc::new(Counters::default()))
    }

    fn with_counters(bytes: Vec<u8>, counters: Arc<Counters>) -> Self {
        Self {
            bytes,
            position: 0,
            counters,
        }
    }
}

impl Read for CountingCursor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Counters::bump(&self.counters.reads);
        let start = (self.position as usize).min(self.bytes.len());
        let available = self.bytes.len() - start;
        let take = buf.len().min(available);
        buf[..take].copy_from_slice(&self.bytes[start..start + take]);
        self.position += take as u64;
        Ok(take)
    }
}

impl Seek for CountingCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        Counters::bump(&self.counters.seeks);
        let target = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.bytes.len() as u64) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if target < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.position = target as u64;
        Ok(self.position)
    }
}

/// A synthetic encrypted image plus the metadata describing it.
pub(crate) struct Harness {
    image: Vec<u8>,
    metadata: FveMetadata,
}

impl Harness {
    /// Builds the metadata for a fixture.
    fn metadata_for(method_code: u16, encrypted_volume_size: u64) -> FveMetadata {
        FveMetadata {
            encryption_method: EncryptionMethod::from_code(method_code),
            encryption_method_code: method_code,
            volume_guid: [0xAB; 16],
            creation_time: 0,
            entries: Vec::new(),
            encrypted_volume_size,
            volume_header_offset: RELOCATED_OFFSET,
            volume_header_size: HEADER_SIZE,
            metadata_offsets: [META_OFFSET, 0, 0],
            metadata_size: META_SIZE as u32,
        }
    }

    /// Lays ciphertext across the whole image.
    ///
    /// Every sector holds `plaintext_pattern(physical_offset)` encrypted at that
    /// same offset, so a read that lands on the wrong sector or decrypts at the
    /// wrong address produces visibly wrong bytes.
    fn build(metadata: FveMetadata) -> Self {
        let mut image = vec![0u8; IMAGE_LEN as usize];
        let plaintext_from = metadata.encrypted_volume_size;
        for offset in (0..IMAGE_LEN).step_by(512) {
            let mut sector = plaintext_pattern(offset);
            let leave_plain = plaintext_from != 0 && offset >= plaintext_from;
            if !leave_plain {
                cbc128_encrypt(&mut sector, offset);
            }
            let start = offset as usize;
            image[start..start + 512].copy_from_slice(&sector);
        }
        Self { image, metadata }
    }

    /// A whole-volume-encrypted `0x8002` fixture.
    pub(crate) fn standard() -> Self {
        Self::build(Self::metadata_for(0x8002, 0))
    }

    /// A fixture whose conversion stopped at `0x8000`, leaving a plaintext tail.
    pub(crate) fn partially_encrypted() -> Self {
        Self::build(Self::metadata_for(0x8002, 0x8000))
    }

    /// A fixture claiming method `0x8001`, which has no validated decrypt path.
    pub(crate) fn unsupported_method() -> Self {
        Self::build(Self::metadata_for(0x8001, 0))
    }

    fn keys(&self) -> VolumeKeyPackage {
        VolumeKeyPackage::new(FVEK.to_vec(), None)
    }

    /// The shared unlock state, or the error that prevented it.
    pub(crate) fn try_volume(&self) -> Result<Arc<UnlockedVolume>> {
        UnlockedVolume::new(&self.metadata, &self.keys()).map(Arc::new)
    }

    /// The shared unlock state.
    pub(crate) fn volume(&self) -> Arc<UnlockedVolume> {
        self.try_volume().expect("the fixture method is supported")
    }

    /// A fresh cursor over the image.
    pub(crate) fn cursor(&self) -> CountingCursor {
        CountingCursor::new(self.image.clone())
    }

    /// A reader over the image.
    pub(crate) fn reader(&self) -> BitLockerReader<CountingCursor> {
        BitLockerReader::new(self.volume(), self.cursor()).expect("reader opens")
    }

    /// A reader plus the counters recording the I/O its cursor receives.
    pub(crate) fn counting_reader(&self) -> (BitLockerReader<CountingCursor>, Arc<Counters>) {
        let counters = Arc::new(Counters::default());
        let cursor = CountingCursor::with_counters(self.image.clone(), counters.clone());
        let reader = BitLockerReader::new(self.volume(), cursor).expect("reader opens");
        // Opening seeks to measure the length; start the count from zero so the
        // assertions describe the reads under test.
        counters
            .reads
            .store(0, std::sync::atomic::Ordering::Relaxed);
        counters
            .seeks
            .store(0, std::sync::atomic::Ordering::Relaxed);
        (reader, counters)
    }
}
