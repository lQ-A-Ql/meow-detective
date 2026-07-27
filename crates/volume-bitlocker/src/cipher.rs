//! The FVEK-keyed sector transform.
//!
//! Derived from `bitlocker-core`'s `crypto` module sector path (see `../NOTICE`).
//!
//! One [`SectorCipher`] is built per unlocked volume and decrypts sectors for the
//! lifetime of that unlock. It holds expanded AES key schedules, so it is a
//! secret-bearing type: no `Debug`, no `Clone`, no `Serialize`.
//!
//! # Three details that fail silently
//!
//! Getting any of these wrong yields 512 plausible-looking bytes and no error,
//! which is why the public oracles rather than a round-trip are the real check.
//!
//! 1. The CBC IV is `AES-ECB(FVEK, LE128(offset))`, not the offset itself.
//! 2. The diffuser sector key is two ECB blocks under the *tweak* key over the
//!    same `LE128(offset)`, the second with byte 15 forced to `0x80`.
//! 3. XTS keys its data unit off the **sector number** (`offset / 512`), while
//!    CBC keys off the **byte offset**. Feeding a byte offset to XTS decrypts to
//!    garbage without complaint.

use aes::cipher::block_padding::NoPadding;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{
    BlockCipher, BlockDecrypt, BlockDecryptMut, BlockEncrypt, BlockSizeUser, KeyInit, KeyIvInit,
};
use aes::{Aes128, Aes256};
use xts_mode::{get_tweak_default, Xts128};

use crate::diffuser;
use crate::error::{BitLockerError, Result};
use crate::method::EncryptionMethod;
use crate::secret::VolumeKeyPackage;

/// BitLocker's fixed logical sector size for cipher addressing.
///
/// This is the cipher's data-unit size, not the volume's `bytes_per_sector`. The
/// format keys every transform off 512-byte units regardless of the BPB value.
pub(crate) const CIPHER_SECTOR_SIZE: usize = 512;

/// The keyed transform for one volume, selected by encryption method.
///
/// Each variant owns exactly the key material its cipher needs. The size skew
/// between variants is deliberate: the pre-expanded key schedules live for the
/// whole unlock while `decrypt_sector` runs per sector, so boxing a variant would
/// put heap indirection on the hot path for nothing.
#[allow(clippy::large_enum_variant)]
enum SectorTransform {
    /// AES-128-CBC. `tweak` present means the Elephant Diffuser also applies
    /// (method `0x8000`); absent means the CBC plaintext is final (`0x8002`).
    Cbc128 {
        fvek: [u8; 16],
        fvek_ecb: Aes128,
        tweak: Option<Aes128>,
    },
    /// AES-256-CBC, no diffuser (method `0x8003`).
    Cbc256 { fvek: [u8; 32], fvek_ecb: Aes256 },
    /// XTS-AES-128 (method `0x8004`).
    Xts128 { xts: Xts128<Aes128> },
    /// XTS-AES-256 (method `0x8005`).
    Xts256 { xts: Xts128<Aes256> },
}

/// The volume sector cipher.
///
/// Deliberately not `Debug`, `Clone`, or `Serialize`: it holds expanded key
/// schedules. `scripts/check-bitlocker-credential-guard.ps1` enforces that.
///
/// # Key-schedule residue
///
/// The FVEK bytes in [`VolumeKeyPackage`] are zeroized on drop, but `aes` 0.8
/// exposes no `zeroize` feature, so the expanded schedules inside `Aes128` and
/// `Aes256` are not wiped when this drops. Moving to `aes` 0.9 would fix that and
/// would also break `xts-mode` 0.5, which needs `cipher` 0.4. The residue is
/// accepted for v1 and recorded in `docs/bitlocker-dependency-decision.md`; it is
/// bounded by keeping one cipher per unlocked volume rather than per read.
pub struct SectorCipher {
    transform: SectorTransform,
}

impl SectorCipher {
    /// Builds the cipher for `method` from a verified key package.
    ///
    /// # Errors
    ///
    /// [`BitLockerError::UnsupportedEncryptionMethod`] when the method has no
    /// validated decrypt path, and [`BitLockerError::MetadataUnreadable`] when the
    /// key package length does not match what the method requires — a mismatch
    /// there would otherwise silently key the cipher off the wrong bytes.
    pub fn new(method: EncryptionMethod, keys: &VolumeKeyPackage) -> Result<Self> {
        let expected = method
            .fvek_len()
            .ok_or(BitLockerError::UnsupportedEncryptionMethod {
                code: method.code(),
                label: method.label(),
            })?;
        let fvek = keys.expose_fvek();
        if fvek.len() != expected {
            return Err(BitLockerError::MetadataUnreadable {
                reason: format!(
                    "{} needs a {expected}-byte FVEK, key package holds {}",
                    method.label(),
                    fvek.len()
                ),
            });
        }

        let transform = match method {
            EncryptionMethod::Aes128CbcDiffuser => {
                let tweak =
                    keys.expose_tweak()
                        .ok_or_else(|| BitLockerError::MetadataUnreadable {
                            reason: "the Elephant Diffuser methods require a tweak key".to_string(),
                        })?;
                let tweak = take_key::<16>(tweak, "diffuser tweak")?;
                let fvek = take_key::<16>(fvek, "FVEK")?;
                SectorTransform::Cbc128 {
                    fvek,
                    fvek_ecb: Aes128::new(GenericArray::from_slice(&fvek)),
                    tweak: Some(Aes128::new(GenericArray::from_slice(&tweak))),
                }
            }
            EncryptionMethod::Aes128Cbc => {
                let fvek = take_key::<16>(fvek, "FVEK")?;
                SectorTransform::Cbc128 {
                    fvek,
                    fvek_ecb: Aes128::new(GenericArray::from_slice(&fvek)),
                    tweak: None,
                }
            }
            EncryptionMethod::Aes256Cbc => {
                let fvek = take_key::<32>(fvek, "FVEK")?;
                SectorTransform::Cbc256 {
                    fvek,
                    fvek_ecb: Aes256::new(GenericArray::from_slice(&fvek)),
                }
            }
            EncryptionMethod::XtsAes128 => {
                // The 32-byte FVEK is two AES-128 keys: data then tweak.
                let data = Aes128::new(GenericArray::from_slice(&fvek[0..16]));
                let tweak = Aes128::new(GenericArray::from_slice(&fvek[16..32]));
                SectorTransform::Xts128 {
                    xts: Xts128::new(data, tweak),
                }
            }
            EncryptionMethod::XtsAes256 => {
                // The 64-byte FVEK is two AES-256 keys: data then tweak.
                let data = Aes256::new(GenericArray::from_slice(&fvek[0..32]));
                let tweak = Aes256::new(GenericArray::from_slice(&fvek[32..64]));
                SectorTransform::Xts256 {
                    xts: Xts128::new(data, tweak),
                }
            }
            EncryptionMethod::Aes256CbcDiffuser | EncryptionMethod::Unknown(_) => {
                // Unreachable: `fvek_len` already returned None for these. Kept as
                // an explicit arm so adding a method is a compile error here.
                return Err(BitLockerError::UnsupportedEncryptionMethod {
                    code: method.code(),
                    label: method.label(),
                });
            }
        };
        Ok(Self { transform })
    }

    /// Decrypts one sector, given its **physical** volume byte offset.
    ///
    /// The offset is the cipher's address, so a relocated sector must be
    /// decrypted at the offset it is physically stored at, not the logical one it
    /// presents as. `buf` is decrypted in place; its length should be a non-zero
    /// multiple of 16.
    pub(crate) fn decrypt_sector(&self, buf: &mut [u8], physical_offset: u64) {
        let iv_input = le128(physical_offset);
        match &self.transform {
            SectorTransform::Cbc128 {
                fvek,
                fvek_ecb,
                tweak,
            } => {
                let iv = ecb_block(fvek_ecb, &iv_input);
                let plain_len = cbc_decrypt_in_place::<Aes128>(fvek, &iv, buf);
                if let Some(tweak) = tweak {
                    let sector_key = diffuser_sector_key(tweak, physical_offset);
                    diffuser::decrypt(&mut buf[..plain_len], &sector_key);
                }
            }
            SectorTransform::Cbc256 { fvek, fvek_ecb } => {
                let iv = ecb_block(fvek_ecb, &iv_input);
                let _ = cbc_decrypt_in_place::<Aes256>(fvek, &iv, buf);
            }
            SectorTransform::Xts128 { xts } => xts_decrypt(xts, buf, physical_offset),
            SectorTransform::Xts256 { xts } => xts_decrypt(xts, buf, physical_offset),
        }
    }
}

/// `LE128(offset)`: the 8-byte little-endian offset, zero-padded to 16.
fn le128(offset: u64) -> [u8; 16] {
    let mut block = [0u8; 16];
    block[0..8].copy_from_slice(&offset.to_le_bytes());
    block
}

/// AES-ECB-encrypts one block. Used for the CBC IV and the diffuser sector key.
fn ecb_block<C>(cipher: &C, input: &[u8; 16]) -> [u8; 16]
where
    C: BlockEncrypt + BlockSizeUser<BlockSize = aes::cipher::consts::U16>,
{
    let mut block = GenericArray::clone_from_slice(input);
    cipher.encrypt_block(&mut block);
    block.into()
}

/// The 32-byte diffuser per-sector key.
///
/// `ECB(tweak, LE128(offset)) || ECB(tweak, LE128(offset) with byte 15 = 0x80)`.
fn diffuser_sector_key(tweak: &Aes128, offset: u64) -> [u8; 32] {
    let mut block = le128(offset);
    let lower = ecb_block(tweak, &block);
    block[15] = 0x80;
    let upper = ecb_block(tweak, &block);
    let mut key = [0u8; 32];
    key[0..16].copy_from_slice(&lower);
    key[16..32].copy_from_slice(&upper);
    key
}

/// AES-CBC-decrypts in place with no padding, returning the plaintext length.
fn cbc_decrypt_in_place<C>(fvek: &[u8], iv: &[u8; 16], buf: &mut [u8]) -> usize
where
    C: BlockCipher + BlockDecrypt + KeyInit,
{
    let decryptor =
        cbc::Decryptor::<C>::new(GenericArray::from_slice(fvek), GenericArray::from_slice(iv));
    let len = buf.len() - (buf.len() % 16);
    match decryptor.decrypt_padded_mut::<NoPadding>(&mut buf[..len]) {
        Ok(plain) => plain.len(),
        // `len` is a 16-byte multiple, so NoPadding decryption cannot fail. The
        // arm keeps the evidence read path panic-free if that ever changes.
        Err(_) => len,
    }
}

/// XTS-decrypts one sector in place, keyed off the sector number.
fn xts_decrypt<C>(xts: &Xts128<C>, buf: &mut [u8], physical_offset: u64)
where
    C: BlockEncrypt + BlockDecrypt + BlockCipher,
{
    // XTS needs at least one full block. The read path always passes a full
    // sector; the guard keeps a short slice from panicking.
    if buf.len() >= 16 {
        let sector_number = physical_offset / CIPHER_SECTOR_SIZE as u64;
        xts.decrypt_sector(buf, get_tweak_default(u128::from(sector_number)));
    }
}

/// Copies a fixed-size key out of a slice whose length was already checked.
fn take_key<const N: usize>(source: &[u8], what: &str) -> Result<[u8; N]> {
    let slice = source
        .get(0..N)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!("{what} holds {} bytes, need {N}", source.len()),
        })?;
    let mut key = [0u8; N];
    key.copy_from_slice(slice);
    Ok(key)
}

#[cfg(test)]
#[path = "../tests/unit/cipher/mod.rs"]
mod tests;
