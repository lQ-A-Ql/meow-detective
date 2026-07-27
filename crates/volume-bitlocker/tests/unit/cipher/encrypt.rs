//! The encrypt direction of every supported sector transform.
//!
//! Production is read-only and never encrypts, so this lives entirely in the test
//! tree. It exists to build ciphertext the decrypt path must invert.
//!
//! A passing round-trip here proves only that these two directions agree with
//! each other, **not** that either matches BitLocker. The real proof is the public
//! oracle set in `docs/bitlocker-volume-layer-design.md` section 4. What these
//! tests do catch is an edit to the decrypt path that changes its behaviour.

use aes::cipher::block_padding::NoPadding;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit};
use aes::{Aes128, Aes256};
use xts_mode::{get_tweak_default, Xts128};

use crate::diffuser::{from_words, to_words, wrapping_back};

/// Diffuser A rotation amounts.
const ROTATIONS_A: [u32; 4] = [9, 0, 13, 0];
/// Diffuser B rotation amounts.
const ROTATIONS_B: [u32; 4] = [0, 10, 0, 25];

/// Diffuser A, encrypt direction: descending indices, `wrapping_sub`.
fn diffuser_a_encrypt(sector: &mut [u8]) {
    let mut words = to_words(sector);
    let count = words.len();
    if count == 0 {
        return;
    }
    for _ in 0..5 {
        for index in (0..count).rev() {
            let near = words[wrapping_back(index, 2, count)];
            let far = words[wrapping_back(index, 5, count)].rotate_left(ROTATIONS_A[index % 4]);
            words[index] = words[index].wrapping_sub(near ^ far);
        }
    }
    from_words(&words, sector);
}

/// Diffuser B, encrypt direction: descending indices, `wrapping_sub`.
fn diffuser_b_encrypt(sector: &mut [u8]) {
    let mut words = to_words(sector);
    let count = words.len();
    if count == 0 {
        return;
    }
    for _ in 0..3 {
        for index in (0..count).rev() {
            let near = words[(index + 2) % count];
            let far = words[(index + 5) % count].rotate_left(ROTATIONS_B[index % 4]);
            words[index] = words[index].wrapping_sub(near ^ far);
        }
    }
    from_words(&words, sector);
}

/// Applies the diffuser stage: sector-key XOR, then Diffuser A, then Diffuser B.
pub(crate) fn diffuser_encrypt(sector: &mut [u8], sector_key: &[u8; 32]) {
    for (index, byte) in sector.iter_mut().enumerate() {
        *byte ^= sector_key[index % 32];
    }
    diffuser_a_encrypt(sector);
    diffuser_b_encrypt(sector);
}

/// `LE128(offset)`, matching the production helper.
pub(crate) fn le128(offset: u64) -> [u8; 16] {
    let mut block = [0u8; 16];
    block[0..8].copy_from_slice(&offset.to_le_bytes());
    block
}

/// AES-ECB-encrypts one block.
fn ecb_block<C>(cipher: &C, input: &[u8; 16]) -> [u8; 16]
where
    C: BlockEncrypt + aes::cipher::BlockSizeUser<BlockSize = aes::cipher::consts::U16>,
{
    let mut block = GenericArray::clone_from_slice(input);
    cipher.encrypt_block(&mut block);
    block.into()
}

/// The 32-byte diffuser per-sector key.
pub(crate) fn diffuser_sector_key(tweak_key: &[u8; 16], offset: u64) -> [u8; 32] {
    let tweak = Aes128::new(GenericArray::from_slice(tweak_key));
    let mut block = le128(offset);
    let lower = ecb_block(&tweak, &block);
    block[15] = 0x80;
    let upper = ecb_block(&tweak, &block);
    let mut key = [0u8; 32];
    key[0..16].copy_from_slice(&lower);
    key[16..32].copy_from_slice(&upper);
    key
}

/// AES-128-CBC-encrypts a sector in place, with the BitLocker-derived IV.
pub(crate) fn cbc128_encrypt(fvek: &[u8; 16], sector: &mut [u8], offset: u64) {
    let iv = ecb_block(&Aes128::new(GenericArray::from_slice(fvek)), &le128(offset));
    let len = sector.len() - (sector.len() % 16);
    cbc::Encryptor::<Aes128>::new(
        GenericArray::from_slice(fvek),
        GenericArray::from_slice(&iv),
    )
    .encrypt_padded_mut::<NoPadding>(&mut sector[..len], len)
    .expect("NoPadding CBC over a 16-byte multiple cannot fail");
}

/// AES-256-CBC-encrypts a sector in place, with the BitLocker-derived IV.
pub(crate) fn cbc256_encrypt(fvek: &[u8; 32], sector: &mut [u8], offset: u64) {
    let iv = ecb_block(&Aes256::new(GenericArray::from_slice(fvek)), &le128(offset));
    let len = sector.len() - (sector.len() % 16);
    cbc::Encryptor::<Aes256>::new(
        GenericArray::from_slice(fvek),
        GenericArray::from_slice(&iv),
    )
    .encrypt_padded_mut::<NoPadding>(&mut sector[..len], len)
    .expect("NoPadding CBC over a 16-byte multiple cannot fail");
}

/// XTS-AES-128-encrypts a sector in place, keyed off the sector number.
pub(crate) fn xts128_encrypt(fvek: &[u8; 32], sector: &mut [u8], offset: u64) {
    let xts = Xts128::new(
        Aes128::new(GenericArray::from_slice(&fvek[0..16])),
        Aes128::new(GenericArray::from_slice(&fvek[16..32])),
    );
    xts.encrypt_sector(sector, get_tweak_default(u128::from(offset / 512)));
}

/// XTS-AES-256-encrypts a sector in place, keyed off the sector number.
pub(crate) fn xts256_encrypt(fvek: &[u8; 64], sector: &mut [u8], offset: u64) {
    let xts = Xts128::new(
        Aes256::new(GenericArray::from_slice(&fvek[0..32])),
        Aes256::new(GenericArray::from_slice(&fvek[32..64])),
    );
    xts.encrypt_sector(sector, get_tweak_default(u128::from(offset / 512)));
}
