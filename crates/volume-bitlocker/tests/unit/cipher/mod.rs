#[path = "encrypt.rs"]
mod encrypt;

use super::*;
use crate::method::EncryptionMethod;
use crate::secret::VolumeKeyPackage;
use encrypt::{
    cbc128_encrypt, cbc256_encrypt, diffuser_encrypt, diffuser_sector_key, le128, xts128_encrypt,
    xts256_encrypt,
};

/// A recognisable 512-byte plaintext sector.
fn plaintext_sector() -> [u8; CIPHER_SECTOR_SIZE] {
    let mut sector = [0u8; CIPHER_SECTOR_SIZE];
    for (index, byte) in sector.iter_mut().enumerate() {
        *byte = ((index as u32).wrapping_mul(31) ^ 0xA5) as u8;
    }
    sector
}

/// Builds a key of `len` distinct-looking bytes.
fn key_bytes(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|index| seed ^ (index as u8)).collect()
}

/// Encrypts a sector the way a volume using `method` would have stored it.
fn encrypt_for(
    method: EncryptionMethod,
    fvek: &[u8],
    tweak: Option<&[u8]>,
    offset: u64,
) -> [u8; CIPHER_SECTOR_SIZE] {
    let mut sector = plaintext_sector();
    match method {
        EncryptionMethod::Aes128CbcDiffuser => {
            let tweak_key: [u8; 16] = tweak
                .expect("the diffuser method needs a tweak key")
                .try_into()
                .expect("tweak key is 16 bytes");
            let sector_key = diffuser_sector_key(&tweak_key, offset);
            diffuser_encrypt(&mut sector, &sector_key);
            cbc128_encrypt(&fvek.try_into().expect("16-byte FVEK"), &mut sector, offset);
        }
        EncryptionMethod::Aes128Cbc => {
            cbc128_encrypt(&fvek.try_into().expect("16-byte FVEK"), &mut sector, offset);
        }
        EncryptionMethod::Aes256Cbc => {
            cbc256_encrypt(&fvek.try_into().expect("32-byte FVEK"), &mut sector, offset);
        }
        EncryptionMethod::XtsAes128 => {
            xts128_encrypt(&fvek.try_into().expect("32-byte FVEK"), &mut sector, offset);
        }
        EncryptionMethod::XtsAes256 => {
            xts256_encrypt(&fvek.try_into().expect("64-byte FVEK"), &mut sector, offset);
        }
        other => panic!("{other:?} has no encrypt path"),
    }
    sector
}

/// The five methods v1 decrypts, with FVEK length and tweak requirement.
fn decryptable_methods() -> Vec<(EncryptionMethod, usize, bool)> {
    vec![
        (EncryptionMethod::Aes128CbcDiffuser, 16, true),
        (EncryptionMethod::Aes128Cbc, 16, false),
        (EncryptionMethod::Aes256Cbc, 32, false),
        (EncryptionMethod::XtsAes128, 32, false),
        (EncryptionMethod::XtsAes256, 64, false),
    ]
}

fn package_for(fvek_len: usize, with_tweak: bool) -> VolumeKeyPackage {
    VolumeKeyPackage::new(
        key_bytes(fvek_len, 0x11),
        with_tweak.then(|| key_bytes(16, 0x22)),
    )
}

#[test]
fn every_supported_method_round_trips_at_a_nonzero_offset() {
    // Offset 0 would hide an IV or tweak that ignores the offset entirely, so the
    // round trip runs at a sector well into the volume.
    let offset = 0x0002_2000u64;
    for (method, fvek_len, with_tweak) in decryptable_methods() {
        let keys = package_for(fvek_len, with_tweak);
        let mut ciphertext = encrypt_for(method, keys.expose_fvek(), keys.expose_tweak(), offset);
        assert_ne!(
            ciphertext,
            plaintext_sector(),
            "{method:?} produced identity ciphertext"
        );

        let cipher = SectorCipher::new(method, &keys).expect("cipher builds");
        cipher.decrypt_sector(&mut ciphertext, offset);
        assert_eq!(
            ciphertext,
            plaintext_sector(),
            "{method:?} did not round-trip at offset {offset:#x}"
        );
    }
}

#[test]
fn every_supported_method_round_trips_at_offset_zero() {
    for (method, fvek_len, with_tweak) in decryptable_methods() {
        let keys = package_for(fvek_len, with_tweak);
        let mut ciphertext = encrypt_for(method, keys.expose_fvek(), keys.expose_tweak(), 0);
        let cipher = SectorCipher::new(method, &keys).expect("cipher builds");
        cipher.decrypt_sector(&mut ciphertext, 0);
        assert_eq!(ciphertext, plaintext_sector(), "{method:?} at offset 0");
    }
}

#[test]
fn the_offset_is_part_of_every_transform() {
    // Decrypting at the wrong offset must produce garbage. If it succeeded, the
    // cipher would be ignoring its sector address, and every relocated sector in
    // the volume would decrypt wrong without raising anything.
    for (method, fvek_len, with_tweak) in decryptable_methods() {
        let keys = package_for(fvek_len, with_tweak);
        let mut ciphertext = encrypt_for(method, keys.expose_fvek(), keys.expose_tweak(), 0x1000);
        let cipher = SectorCipher::new(method, &keys).expect("cipher builds");
        cipher.decrypt_sector(&mut ciphertext, 0x1200);
        assert_ne!(
            ciphertext,
            plaintext_sector(),
            "{method:?} decrypted correctly at the wrong offset"
        );
    }
}

#[test]
fn xts_keys_off_the_sector_number_not_the_byte_offset() {
    // Two byte offsets inside one 512-byte sector share a sector number, so XTS
    // must treat them identically. Crossing this axis with CBC's byte-offset IV
    // decrypts to garbage with no error.
    let keys = package_for(32, false);
    let cipher = SectorCipher::new(EncryptionMethod::XtsAes128, &keys).expect("cipher builds");

    let mut within_sector = encrypt_for(
        EncryptionMethod::XtsAes128,
        keys.expose_fvek(),
        None,
        0x1000,
    );
    cipher.decrypt_sector(&mut within_sector, 0x1000 + 16);
    assert_eq!(
        within_sector,
        plaintext_sector(),
        "offsets within one sector must share an XTS tweak"
    );

    let mut other_sector = encrypt_for(
        EncryptionMethod::XtsAes128,
        keys.expose_fvek(),
        None,
        0x1000,
    );
    cipher.decrypt_sector(&mut other_sector, 0x1200);
    assert_ne!(other_sector, plaintext_sector());
}

#[test]
fn cbc_keys_off_the_byte_offset_not_the_sector_number() {
    let keys = package_for(16, false);
    let cipher = SectorCipher::new(EncryptionMethod::Aes128Cbc, &keys).expect("cipher builds");
    let mut sector = encrypt_for(
        EncryptionMethod::Aes128Cbc,
        keys.expose_fvek(),
        None,
        0x1000,
    );
    cipher.decrypt_sector(&mut sector, 0x1000 + 16);
    assert_ne!(
        sector,
        plaintext_sector(),
        "CBC must distinguish byte offsets inside one sector"
    );
}

#[test]
fn a_diffused_sector_does_not_decrypt_as_plain_cbc() {
    // 0x8000 and 0x8002 share the CBC layer and a 16-byte FVEK; only the diffuser
    // separates them. If this passed, method dispatch would not be selecting the
    // diffuser stage at all.
    let fvek = key_bytes(16, 0x11);
    let with_diffuser = VolumeKeyPackage::new(fvek.clone(), Some(key_bytes(16, 0x22)));
    let without = VolumeKeyPackage::new(fvek, None);

    let mut ciphertext = encrypt_for(
        EncryptionMethod::Aes128CbcDiffuser,
        with_diffuser.expose_fvek(),
        with_diffuser.expose_tweak(),
        0x1000,
    );
    SectorCipher::new(EncryptionMethod::Aes128Cbc, &without)
        .expect("cipher builds")
        .decrypt_sector(&mut ciphertext, 0x1000);
    assert_ne!(ciphertext, plaintext_sector());
}

#[test]
fn the_tweak_key_is_part_of_the_diffuser() {
    let fvek = key_bytes(16, 0x11);
    let right = VolumeKeyPackage::new(fvek.clone(), Some(key_bytes(16, 0x22)));
    let wrong = VolumeKeyPackage::new(fvek, Some(key_bytes(16, 0x23)));

    let mut ciphertext = encrypt_for(
        EncryptionMethod::Aes128CbcDiffuser,
        right.expose_fvek(),
        right.expose_tweak(),
        0x1000,
    );
    SectorCipher::new(EncryptionMethod::Aes128CbcDiffuser, &wrong)
        .expect("cipher builds")
        .decrypt_sector(&mut ciphertext, 0x1000);
    assert_ne!(ciphertext, plaintext_sector());
}

#[test]
fn xts_key_halves_are_not_interchangeable() {
    // The 32-byte XTS-128 FVEK is data key then tweak key. Swapping the halves
    // still builds a valid cipher and still decrypts silently to garbage.
    let keys = package_for(32, false);
    let mut swapped = keys.expose_fvek().to_vec();
    swapped.rotate_left(16);
    let swapped = VolumeKeyPackage::new(swapped, None);

    let mut ciphertext = encrypt_for(
        EncryptionMethod::XtsAes128,
        keys.expose_fvek(),
        None,
        0x1000,
    );
    SectorCipher::new(EncryptionMethod::XtsAes128, &swapped)
        .expect("cipher builds")
        .decrypt_sector(&mut ciphertext, 0x1000);
    assert_ne!(ciphertext, plaintext_sector());
}

#[test]
fn cipher_rejects_an_unsupported_method() {
    let keys = package_for(32, false);
    for method in [
        EncryptionMethod::Aes256CbcDiffuser,
        EncryptionMethod::Unknown(0x8009),
    ] {
        let error = match SectorCipher::new(method, &keys) {
            Ok(_) => panic!("{method:?} must not build a cipher"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_METHOD");
    }
}

#[test]
fn cipher_rejects_a_key_package_of_the_wrong_length() {
    // A length mismatch would key the cipher off the wrong bytes and decrypt
    // silently to garbage, so it has to fail at build time.
    for (method, fvek_len, with_tweak) in decryptable_methods() {
        let short = VolumeKeyPackage::new(
            key_bytes(fvek_len - 8, 0x11),
            with_tweak.then(|| key_bytes(16, 0x22)),
        );
        let error = match SectorCipher::new(method, &short) {
            Ok(_) => panic!("{method:?} accepted a short FVEK"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
        assert!(error.to_string().contains("need"));
    }
}

#[test]
fn the_diffuser_method_requires_a_tweak_key() {
    let no_tweak = VolumeKeyPackage::new(key_bytes(16, 0x11), None);
    let error = match SectorCipher::new(EncryptionMethod::Aes128CbcDiffuser, &no_tweak) {
        Ok(_) => panic!("the diffuser method must not build without a tweak"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
    assert!(error.to_string().contains("tweak"));
}

#[test]
fn le128_places_the_offset_little_endian_and_zero_pads() {
    assert_eq!(
        le128(0x0102_0304_0506_0708),
        [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn the_diffuser_sector_key_halves_differ() {
    // Both halves are ECB over the same block, distinguished only by byte 15 being
    // forced to 0x80. If they matched, that byte was not being set and the key
    // would be half as wide as the format specifies.
    let key = diffuser_sector_key(&[0x22u8; 16], 0x1000);
    assert_ne!(key[0..16], key[16..32]);
}

#[test]
fn decrypting_a_short_slice_does_not_panic() {
    // The read path always passes a full sector, but every method must stay
    // panic-free on a sub-block slice.
    for (method, fvek_len, with_tweak) in decryptable_methods() {
        let keys = package_for(fvek_len, with_tweak);
        let cipher = SectorCipher::new(method, &keys).expect("cipher builds");
        for len in [0usize, 1, 8, 15, 16, 17] {
            let mut buffer = vec![0xABu8; len];
            cipher.decrypt_sector(&mut buffer, 0x1000);
        }
    }
}
