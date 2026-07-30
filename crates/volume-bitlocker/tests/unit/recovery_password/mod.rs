use aes::Aes256;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::AeadInPlace;
use ccm::consts::{U12, U16};
use ccm::{Ccm, KeyInit};

use super::*;
use super::{formatter::format_material, material::RecoveryPasswordMaterial};
use crate::metadata::{
    ENTRY_TYPE_VMK, PROTECTION_RECOVERY, VALUE_TYPE_AES_CCM, VALUE_TYPE_STRETCH, VALUE_TYPE_VMK,
};
use crate::{EncryptionMethod, FveMetadata, MetadataEntry, RecoveredVmk};

type TestCcm = Ccm<Aes256, U16, U12>;

const PROTECTOR_GUID: [u8; 16] = [0xAB; 16];
const VOLUME_GUID: [u8; 16] = [0xCD; 16];

fn decode_hex<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), N * 2);
    let mut bytes = [0u8; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex fixture");
    }
    bytes
}

fn entry(entry_type: u16, value_type: u16, data: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(8 + data.len());
    encoded.extend_from_slice(&((8 + data.len()) as u16).to_le_bytes());
    encoded.extend_from_slice(&entry_type.to_le_bytes());
    encoded.extend_from_slice(&value_type.to_le_bytes());
    encoded.extend_from_slice(&1u16.to_le_bytes());
    encoded.extend_from_slice(data);
    encoded
}

fn encrypt_reverse_datum(vmk: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let nonce = [0x42; 12];
    let cipher = <TestCcm as KeyInit>::new(GenericArray::from_slice(vmk));
    let mut ciphertext = plaintext.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(GenericArray::from_slice(&nonce), &[], &mut ciphertext)
        .expect("synthetic CCM encryption");
    let mut encoded = Vec::with_capacity(28 + ciphertext.len());
    encoded.extend_from_slice(&nonce);
    encoded.extend_from_slice(&tag);
    encoded.extend_from_slice(&ciphertext);
    encoded
}

fn key_datum(material: [u8; 16]) -> [u8; 28] {
    let mut plaintext = [0u8; 28];
    plaintext[0..2].copy_from_slice(&28u16.to_le_bytes());
    plaintext[4..6].copy_from_slice(&1u16.to_le_bytes());
    plaintext[8..10].copy_from_slice(&0x1000u16.to_le_bytes());
    plaintext[12..28].copy_from_slice(&material);
    plaintext
}

fn recovery_entry(guid: [u8; 16], reverse_data: &[u8]) -> MetadataEntry {
    let mut stretch = vec![0u8; 20];
    stretch.extend_from_slice(&entry(0, VALUE_TYPE_AES_CCM, reverse_data));
    let mut data = vec![0u8; 28];
    data[0..16].copy_from_slice(&guid);
    data[26..28].copy_from_slice(&PROTECTION_RECOVERY.to_le_bytes());
    data.extend_from_slice(&entry(0, VALUE_TYPE_STRETCH, &stretch));
    MetadataEntry {
        entry_type: ENTRY_TYPE_VMK,
        value_type: VALUE_TYPE_VMK,
        version: 1,
        data,
    }
}

fn metadata_for(entries: Vec<MetadataEntry>) -> FveMetadata {
    FveMetadata {
        encryption_method: EncryptionMethod::XtsAes128,
        encryption_method_code: 0x8004,
        volume_guid: VOLUME_GUID,
        creation_time: 0x01D9_0000_0000_0000,
        entries,
        encrypted_volume_size: 0,
        volume_header_offset: 0,
        volume_header_size: 0,
        metadata_offsets: [0x1000, 0x2000, 0x3000],
        metadata_size: 0x400,
    }
}

fn identity() -> RecoveryPasswordProtectorIdentity {
    RecoveryPasswordProtectorIdentity::from_guid(PROTECTOR_GUID)
}

fn recovery_error(
    result: Result<RecoveredRecoveryPassword, RecoveryPasswordRecoveryError>,
) -> RecoveryPasswordRecoveryError {
    match result {
        Ok(_) => panic!("recovery unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn public_vmk2rk_oracle_authenticates_and_formats_exactly() {
    let vmk = decode_hex::<32>("7e63180cb55e15fa62ff4a2cac1bec4ca2ae145b8b92b59b8674e1f2169bcc8d");
    let mut reverse = Vec::new();
    reverse.extend_from_slice(&decode_hex::<12>("a0d6eb7c2b03dc010f000000"));
    reverse.extend_from_slice(&decode_hex::<16>("709fbe999a0b37582d230a7cdbc40ba0"));
    reverse.extend_from_slice(&decode_hex::<28>(
        "487c4e3f137759409f626bae6e39d24e59d283b6b1e19db6678646ce",
    ));
    let metadata = metadata_for(vec![recovery_entry(PROTECTOR_GUID, &reverse)]);
    let recovered = recover_recovery_password(&metadata, identity(), &RecoveredVmk::new(vmk))
        .expect("public VMK oracle must authenticate");

    assert_eq!(
        recovered.password().expose_for_authorized_reveal(),
        "357302-074998-135157-539968-349327-417395-032670-426536"
    );
    assert_eq!(
        recovered.provenance().volume_guid(),
        "cdcdcdcd-cdcd-cdcd-cdcd-cdcdcdcdcdcd"
    );
    assert_eq!(
        recovered.provenance().protector_guid(),
        "abababab-abab-abab-abab-abababababab"
    );
    assert_eq!(recovered.provenance().metadata_fingerprint().len(), 32);
    assert_eq!(recovered.provenance().reverse_datum_fingerprint().len(), 32);
}

#[test]
fn wrong_vmk_and_tampered_tag_fail_authentication() {
    let vmk = [0x55; 32];
    let reverse = encrypt_reverse_datum(&vmk, &key_datum([0x11; 16]));
    let metadata = metadata_for(vec![recovery_entry(PROTECTOR_GUID, &reverse)]);

    let wrong = recovery_error(recover_recovery_password(
        &metadata,
        identity(),
        &RecoveredVmk::new([0x56; 32]),
    ));
    assert_eq!(wrong, RecoveryPasswordRecoveryError::AuthenticationFailed);

    let mut tampered = reverse;
    tampered[12] ^= 1;
    let metadata = metadata_for(vec![recovery_entry(PROTECTOR_GUID, &tampered)]);
    let error = recovery_error(recover_recovery_password(
        &metadata,
        identity(),
        &RecoveredVmk::new(vmk),
    ));
    assert_eq!(error, RecoveryPasswordRecoveryError::AuthenticationFailed);
}

#[test]
fn authenticated_plaintext_requires_the_exact_key_datum_header() {
    let vmk = [0x66; 32];
    let mut plaintext = key_datum([0x22; 16]);
    plaintext[8] = 0x01;
    let reverse = encrypt_reverse_datum(&vmk, &plaintext);
    let metadata = metadata_for(vec![recovery_entry(PROTECTOR_GUID, &reverse)]);

    let error = recovery_error(recover_recovery_password(
        &metadata,
        identity(),
        &RecoveredVmk::new(vmk),
    ));
    assert!(matches!(
        error,
        RecoveryPasswordRecoveryError::InvalidMaterial { .. }
    ));
}

#[test]
fn duplicate_reverse_data_are_rejected() {
    let vmk = [0x77; 32];
    let reverse = encrypt_reverse_datum(&vmk, &key_datum([0x33; 16]));
    let aes = entry(0, VALUE_TYPE_AES_CCM, &reverse);
    let mut stretch = vec![0u8; 20];
    stretch.extend_from_slice(&aes);
    stretch.extend_from_slice(&aes);
    let mut data = vec![0u8; 28];
    data[0..16].copy_from_slice(&PROTECTOR_GUID);
    data[26..28].copy_from_slice(&PROTECTION_RECOVERY.to_le_bytes());
    data.extend_from_slice(&entry(0, VALUE_TYPE_STRETCH, &stretch));
    let metadata = metadata_for(vec![MetadataEntry {
        entry_type: ENTRY_TYPE_VMK,
        value_type: VALUE_TYPE_VMK,
        version: 1,
        data,
    }]);

    let error = recovery_error(recover_recovery_password(
        &metadata,
        identity(),
        &RecoveredVmk::new(vmk),
    ));
    assert!(matches!(
        error,
        RecoveryPasswordRecoveryError::MalformedProtector { .. }
    ));
}

#[test]
fn inventory_uses_exact_recovery_protector_code() {
    let reverse = vec![0u8; 56];
    let recovery = recovery_entry(PROTECTOR_GUID, &reverse);
    let mut near_match = recovery_entry([0xEF; 16], &reverse);
    near_match.data[26..28].copy_from_slice(&0x0801u16.to_le_bytes());
    let metadata = metadata_for(vec![recovery, near_match]);

    assert_eq!(
        recovery_password_protectors(&metadata).expect("protector inventory"),
        vec![identity()]
    );
}

#[test]
fn duplicate_selected_protector_identity_is_rejected() {
    let vmk = [0x88; 32];
    let reverse = encrypt_reverse_datum(&vmk, &key_datum([0x44; 16]));
    let metadata = metadata_for(vec![
        recovery_entry(PROTECTOR_GUID, &reverse),
        recovery_entry(PROTECTOR_GUID, &reverse),
    ]);

    let error = recovery_error(recover_recovery_password(
        &metadata,
        identity(),
        &RecoveredVmk::new(vmk),
    ));
    assert_eq!(error, RecoveryPasswordRecoveryError::AmbiguousProtector);
}

#[test]
fn formatter_preserves_six_digit_group_width() {
    let material = key_datum([0u8; 16]);
    let parsed = RecoveryPasswordMaterial::parse(&material).expect("valid key datum");
    let password = format_material(&parsed);
    assert_eq!(
        password.expose_for_authorized_reveal(),
        "000000-000000-000000-000000-000000-000000-000000-000000"
    );
}

#[test]
fn reverse_datum_is_selected_by_size_among_multiple_aes_ccm_entries() {
    // Real protectors nest two AES-CCM entries under the stretch key: the
    // 72-byte VMK wrapped by the stretched credential and the 56-byte
    // recovery material wrapped by the plaintext VMK. Recovery must select
    // the 56-byte entry instead of failing on duplication.
    let vmk = [0x77; 32];
    let reverse = encrypt_reverse_datum(&vmk, &key_datum([0x33; 16]));
    let wrapped_vmk = vec![0xAA; 72];
    let mut stretch = vec![0u8; 20];
    stretch.extend_from_slice(&entry(0, VALUE_TYPE_AES_CCM, &wrapped_vmk));
    stretch.extend_from_slice(&entry(0, VALUE_TYPE_AES_CCM, &reverse));
    let mut data = vec![0u8; 28];
    data[0..16].copy_from_slice(&PROTECTOR_GUID);
    data[26..28].copy_from_slice(&PROTECTION_RECOVERY.to_le_bytes());
    data.extend_from_slice(&entry(0, VALUE_TYPE_STRETCH, &stretch));
    let metadata = metadata_for(vec![MetadataEntry {
        entry_type: ENTRY_TYPE_VMK,
        value_type: VALUE_TYPE_VMK,
        version: 1,
        data,
    }]);

    let recovered = recover_recovery_password(&metadata, identity(), &RecoveredVmk::new(vmk))
        .expect("the 56-byte reverse datum must be selected among multiple entries");
    let material = RecoveryPasswordMaterial::parse(&key_datum([0x33; 16])).expect("key datum");
    assert_eq!(
        recovered.password().expose_for_authorized_reveal(),
        format_material(&material).expose_for_authorized_reveal(),
    );
}
