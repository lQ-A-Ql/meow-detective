use volume_bitlocker::RecoveredAesKey;
use zeroize::Zeroizing;

use crate::{
    aes_schedule::{
        is_valid_aes_schedule, AES_128_KEY_LEN, AES_128_SCHEDULE_LEN, AES_256_KEY_LEN,
        AES_256_SCHEDULE_LEN,
    },
    pool::scan_pool_tags,
    RawMemoryImage, Result,
};

const BITLOCKER_POOL_TAGS: [BitLockerPoolTag; 4] = [
    BitLockerPoolTag::FveContext,
    BitLockerPoolTag::CngBuffer,
    BitLockerPoolTag::Untagged,
    BitLockerPoolTag::DriverFve,
];
const MAX_SCANNED_ALLOCATION_BYTES: usize = 1024 * 1024;
const SCHEDULE_SCAN_START: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerPoolTag {
    FveContext,
    CngBuffer,
    Untagged,
    DriverFve,
}

impl BitLockerPoolTag {
    const fn bytes(self) -> [u8; 4] {
        match self {
            Self::FveContext => *b"FVEc",
            Self::CngBuffer => *b"Cngb",
            Self::Untagged => *b"None",
            Self::DriverFve => *b"dFVE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AesKeyBits {
    Aes128,
    Aes256,
}

/// A mathematically valid expanded AES schedule found in a bounded BitLocker pool allocation.
///
/// The original key bytes are secret-bearing and therefore this type deliberately has no
/// `Debug`, `Clone`, or serialization implementation. Its owned bytes are zeroized on drop.
pub struct BitLockerKeyCandidate {
    key: RecoveredAesKey,
    bits: AesKeyBits,
    pool_tag: BitLockerPoolTag,
    pool_physical_address: u64,
    schedule_offset: usize,
}

impl BitLockerKeyCandidate {
    #[must_use]
    pub fn bits(&self) -> AesKeyBits {
        self.bits
    }

    #[must_use]
    pub fn pool_tag(&self) -> BitLockerPoolTag {
        self.pool_tag
    }

    #[must_use]
    pub fn pool_physical_address(&self) -> u64 {
        self.pool_physical_address
    }

    #[must_use]
    pub fn schedule_offset(&self) -> usize {
        self.schedule_offset
    }

    /// Creates another opaque, zeroizing key for one volume-bound validation attempt.
    #[must_use]
    pub fn recovered_key(&self) -> RecoveredAesKey {
        self.key.copy_for_validation()
    }
}

/// Finds AES key schedules inside validated, bounded allocations carrying known BitLocker tags.
pub fn scan_bitlocker_key_candidates(
    image: &mut RawMemoryImage,
    maximum_allocations_per_tag: usize,
    maximum_candidates: usize,
) -> Result<Vec<BitLockerKeyCandidate>> {
    if maximum_allocations_per_tag == 0 || maximum_candidates == 0 {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    let tags = BITLOCKER_POOL_TAGS.map(BitLockerPoolTag::bytes);
    let allocations = scan_pool_tags(image, &tags, maximum_allocations_per_tag)?;
    for (tag_index, allocation) in allocations {
        let pool_tag = BITLOCKER_POOL_TAGS[tag_index];
        let body_len = allocation
            .allocation_bytes
            .saturating_sub(0x10)
            .min(MAX_SCANNED_ALLOCATION_BYTES as u64) as usize;
        if body_len < SCHEDULE_SCAN_START + AES_128_SCHEDULE_LEN {
            continue;
        }
        let mut body = Zeroizing::new(vec![0u8; body_len]);
        image.read_exact_at(allocation.body_physical_address, &mut body)?;
        scan_allocation(
            &body,
            allocation.header_physical_address,
            pool_tag,
            maximum_candidates,
            &mut candidates,
        );
        if candidates.len() == maximum_candidates {
            return Ok(candidates);
        }
    }
    Ok(candidates)
}

fn scan_allocation(
    body: &[u8],
    pool_physical_address: u64,
    pool_tag: BitLockerPoolTag,
    maximum_candidates: usize,
    candidates: &mut Vec<BitLockerKeyCandidate>,
) {
    scan_schedule_size(
        body,
        AES_128_KEY_LEN,
        AES_128_SCHEDULE_LEN,
        AesKeyBits::Aes128,
        pool_physical_address,
        pool_tag,
        maximum_candidates,
        candidates,
    );
    if candidates.len() < maximum_candidates {
        scan_schedule_size(
            body,
            AES_256_KEY_LEN,
            AES_256_SCHEDULE_LEN,
            AesKeyBits::Aes256,
            pool_physical_address,
            pool_tag,
            maximum_candidates,
            candidates,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_schedule_size(
    body: &[u8],
    key_len: usize,
    schedule_len: usize,
    bits: AesKeyBits,
    pool_physical_address: u64,
    pool_tag: BitLockerPoolTag,
    maximum_candidates: usize,
    candidates: &mut Vec<BitLockerKeyCandidate>,
) {
    let Some(last_start) = body.len().checked_sub(schedule_len) else {
        return;
    };
    for offset in SCHEDULE_SCAN_START..=last_start {
        if is_valid_aes_schedule(&body[offset..offset + schedule_len], key_len) {
            candidates.push(BitLockerKeyCandidate {
                key: RecoveredAesKey::new(body[offset..offset + key_len].to_vec())
                    .expect("validated AES key length"),
                bits,
                pool_tag,
                pool_physical_address,
                schedule_offset: offset,
            });
            if candidates.len() == maximum_candidates {
                return;
            }
        }
    }
}
