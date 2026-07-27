//! Synthetic BitLocker volume builders for the unit tests.
//!
//! These tests are self-consistency checks: this file authors the encoder and the
//! crate authors the decoder, so passing proves the pipeline agrees with itself,
//! **not** that it agrees with BitLocker. Real-format proof comes from the
//! env-gated oracles listed in `docs/bitlocker-volume-layer-design.md` section 4.

use aes::Aes256;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::AeadInPlace;
use ccm::consts::{U12, U16};
use ccm::{Ccm, KeyInit};

use crate::kdf::{password_hash, recovery_key_hash, stretch_key_n};
use crate::metadata::{
    ENTRY_TYPE_FVEK, ENTRY_TYPE_VMK, ENTRY_TYPE_VOLUME_HEADER, VALUE_TYPE_AES_CCM,
    VALUE_TYPE_STRETCH, VALUE_TYPE_VMK,
};

/// Value type for a raw key. Test-local: production has no reader for it, since
/// it only appears in the clear-key protector that v1 reports but never uses.
const VALUE_TYPE_KEY: u16 = 0x0001;

type TestCcm = Ccm<Aes256, U16, U12>;

/// Byte offset of the metadata block inside the synthetic images.
pub const META_BLOCK_OFFSET: u64 = 0x1000;
/// Total size of the synthetic images.
pub const IMAGE_SIZE: usize = 0x4000;
/// Byte offset of the relocated volume header inside the synthetic images.
pub const RELOCATED_OFFSET: u64 = 0x3000;

/// The stretch salt every synthetic volume uses.
pub const SALT: [u8; 16] = [0x33u8; 16];

/// Stretch rounds for synthetic volumes.
///
/// The real format mandates 0x100000, which is roughly two seconds per derivation
/// in a debug build. Orchestration tests use this reduced count and pass it to
/// both the builder and the unlock call; a separate test covers the production
/// path at the real count so the constant is not merely asserted in isolation.
pub const TEST_ITERATIONS: u64 = 4;

/// Wraps a plaintext into the on-disk AES-CCM layout `nonce(12) | tag(16) | ct`.
pub fn wrap_key(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let cipher = <TestCcm as KeyInit>::new(GenericArray::from_slice(key));
    let mut buffer = plaintext.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(GenericArray::from_slice(nonce), &[], &mut buffer)
        .expect("in-memory encryption cannot fail");
    let mut out = Vec::with_capacity(28 + buffer.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&tag);
    out.extend_from_slice(&buffer);
    out
}

/// Encodes one metadata entry with its 8-byte header.
pub fn entry(entry_type: u16, value_type: u16, data: &[u8]) -> Vec<u8> {
    let size = (8 + data.len()) as u16;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&entry_type.to_le_bytes());
    out.extend_from_slice(&value_type.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Which credential a synthetic protector is built for.
#[derive(Clone, Copy)]
pub enum Credential<'a> {
    /// A password protector (`0x2000`).
    Password(&'a str),
    /// A recovery-password protector (`0x0800`).
    Recovery(&'a str),
}

impl Credential<'_> {
    /// The on-disk protector code.
    pub fn protection_code(self) -> u16 {
        match self {
            Self::Password(_) => 0x2000,
            Self::Recovery(_) => 0x0800,
        }
    }

    /// The stretch input for this credential.
    pub fn key_hash(self) -> [u8; 32] {
        match self {
            Self::Password(password) => *password_hash(password),
            Self::Recovery(recovery) => {
                *recovery_key_hash(recovery).expect("test recovery password must be valid")
            }
        }
    }
}

/// A synthetic volume plus the key material embedded in it.
pub struct SyntheticVolume {
    /// The image bytes.
    pub image: Vec<u8>,
    /// The FVEK the volume was built with.
    pub fvek: Vec<u8>,
    /// The diffuser tweak key, for the Elephant Diffuser methods.
    pub tweak: Option<Vec<u8>>,
}

/// Options for building a synthetic volume.
pub struct VolumeSpec<'a> {
    /// Encryption-method code written into the metadata header.
    pub method: u16,
    /// FVEK length to embed; normally `EncryptionMethod::fvek_len`.
    pub fvek_len: usize,
    /// Whether to embed a 16-byte diffuser tweak at container offset 44.
    pub with_tweak: bool,
    /// The protectors to build, each with its credential.
    pub protectors: &'a [Credential<'a>],
    /// Extra raw protector codes to add as inventory-only VMK entries.
    pub inventory_only: &'a [u16],
    /// Whether to include the FVEK entry at all.
    pub with_fvek_entry: bool,
    /// Whether to write the `-FVE-FS-` block signature.
    pub with_block_signature: bool,
    /// Stretch rounds used to wrap the VMK. Must match what the unlock call uses.
    pub iterations: u64,
}

impl Default for VolumeSpec<'_> {
    fn default() -> Self {
        Self {
            method: 0x8000,
            fvek_len: 16,
            with_tweak: true,
            protectors: &[],
            inventory_only: &[],
            with_fvek_entry: true,
            with_block_signature: true,
            iterations: TEST_ITERATIONS,
        }
    }
}

/// Builds a synthetic BitLocker To Go volume from `spec`.
pub fn build_volume(spec: &VolumeSpec<'_>) -> SyntheticVolume {
    let vmk = [0x44u8; 32];
    let fvek: Vec<u8> = (0..spec.fvek_len).map(|i| 0x11 ^ (i as u8)).collect();
    let tweak: Option<Vec<u8>> = spec
        .with_tweak
        .then(|| (0..16u8).map(|i| 0x22 ^ i).collect());

    let mut entries = Vec::new();
    entries.extend_from_slice(&volume_header_entry());
    for credential in spec.protectors {
        entries.extend_from_slice(&protector_entry(*credential, &vmk, spec.iterations));
    }
    for code in spec.inventory_only {
        entries.extend_from_slice(&inventory_only_entry(*code));
    }
    if spec.with_fvek_entry {
        entries.extend_from_slice(&fvek_entry(&vmk, &fvek, tweak.as_deref()));
    }

    let image = assemble_image(spec, &entries);
    SyntheticVolume { image, fvek, tweak }
}

/// The relocated volume-header descriptor entry.
fn volume_header_entry() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&RELOCATED_OFFSET.to_le_bytes());
    data.extend_from_slice(&512u64.to_le_bytes());
    entry(ENTRY_TYPE_VOLUME_HEADER, ENTRY_TYPE_VOLUME_HEADER, &data)
}

/// A VMK protector entry carrying a stretch salt and the wrapped VMK.
fn protector_entry(credential: Credential<'_>, vmk: &[u8; 32], iterations: u64) -> Vec<u8> {
    let mut vmk_container = vec![0u8; 44];
    vmk_container[12..44].copy_from_slice(vmk);
    let stretched = stretch_key_n(&credential.key_hash(), &SALT, iterations);
    let wrapped = wrap_key(&stretched, &[0x55; 12], &vmk_container);

    let mut stretch_data = vec![0u8; 4]; // 4-byte method, then the salt
    stretch_data.extend_from_slice(&SALT);

    let mut data = vec![0u8; 28];
    data[26..28].copy_from_slice(&credential.protection_code().to_le_bytes());
    data.extend_from_slice(&entry(0, VALUE_TYPE_STRETCH, &stretch_data));
    data.extend_from_slice(&entry(0, VALUE_TYPE_AES_CCM, &wrapped));
    entry(ENTRY_TYPE_VMK, VALUE_TYPE_VMK, &data)
}

/// A VMK entry with no usable key material, present only in the inventory.
fn inventory_only_entry(code: u16) -> Vec<u8> {
    let mut data = vec![0u8; 28];
    data[26..28].copy_from_slice(&code.to_le_bytes());
    // A KEY property with no real key: enough to be listed, not to unlock.
    data.extend_from_slice(&entry(0, VALUE_TYPE_KEY, &[0u8; 4]));
    entry(ENTRY_TYPE_VMK, VALUE_TYPE_VMK, &data)
}

/// The wrapped-FVEK entry.
fn fvek_entry(vmk: &[u8; 32], fvek: &[u8], tweak: Option<&[u8]>) -> Vec<u8> {
    let mut container = vec![0u8; 12 + fvek.len().max(48)];
    container[12..12 + fvek.len()].copy_from_slice(fvek);
    if let Some(tweak) = tweak {
        if container.len() < 44 + tweak.len() {
            container.resize(44 + tweak.len(), 0);
        }
        container[44..44 + tweak.len()].copy_from_slice(tweak);
    }
    let wrapped = wrap_key(vmk, &[0x66; 12], &container);
    entry(ENTRY_TYPE_FVEK, VALUE_TYPE_AES_CCM, &wrapped)
}

/// Writes the boot sector and metadata block into an image buffer.
fn assemble_image(spec: &VolumeSpec<'_>, entries: &[u8]) -> Vec<u8> {
    let metadata_size = 48 + entries.len();
    let mut image = vec![0u8; IMAGE_SIZE];
    image[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    image[3..11].copy_from_slice(b"MSWIN4.1");
    image[12] = 0x02; // bytes per sector = 512
    image[440..448].copy_from_slice(&META_BLOCK_OFFSET.to_le_bytes());

    let block = META_BLOCK_OFFSET as usize;
    if spec.with_block_signature {
        image[block..block + 8].copy_from_slice(b"-FVE-FS-");
    }
    image[block + 10..block + 12].copy_from_slice(&2u16.to_le_bytes());
    image[block + 28..block + 32].copy_from_slice(&1u32.to_le_bytes());
    image[block + 32..block + 40].copy_from_slice(&META_BLOCK_OFFSET.to_le_bytes());
    image[block + 56..block + 64].copy_from_slice(&RELOCATED_OFFSET.to_le_bytes());

    let header = block + 64;
    image[header..header + 4].copy_from_slice(&(metadata_size as u32).to_le_bytes());
    image[header + 16..header + 32].copy_from_slice(&[0xAB; 16]); // volume GUID
    image[header + 36..header + 38].copy_from_slice(&spec.method.to_le_bytes());
    image[header + 40..header + 48].copy_from_slice(&0x01D9_0000_0000_0000u64.to_le_bytes());
    image[header + 48..header + 48 + entries.len()].copy_from_slice(entries);
    image
}
