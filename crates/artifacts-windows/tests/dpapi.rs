use artifacts_windows::dpapi::{
    decrypt_master_key_file, derive_user_prekeys, parse_dpapi_blob, parse_masterkey_file,
};

fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("valid test hex"))
        .collect()
}

#[test]
fn malformed_dpapi_inputs_are_rejected_without_panicking() {
    assert!(parse_dpapi_blob(&[]).is_err());
    assert!(parse_masterkey_file(&[0; 127]).is_err());
}

#[test]
fn empty_password_prekeys_include_the_sha1_path() {
    let keys = derive_user_prekeys(
        "S-1-5-21-1455520393-2011455520393-2019809541-4133251990-500",
        &hex("31d6cfe0d16ae931b73c59d7e0c089c0")
            .try_into()
            .expect("NT hash length"),
    );
    assert_eq!(keys.len(), 3);
    assert_eq!(
        hex::encode(keys[0]),
        "10d6c3caa8b0600d6396fe24438adcfb7426cbbf"
    );
}

#[test]
fn masterkey_file_parser_matches_wire_header() {
    let file = parse_masterkey_file(&hex(
        "020000000000000000000000650061003900350065006200610038002d0062006100300030002d0034006500310061002d0062003400330066002d00350031006500610033003000310037003100640031003100000000000000000006000000b00000000000000090000000000000001400000000000000000000000000000002000000f42f61ea0c9647bf403819898452089ff84300000e80000010660000e2441dec11a6b6f03ffebba1e71473f78da46a52b12caff7df9925ed6ac89d84050ad15cfe88ec50ece201e5a80eb198909ff8c781510f78859b96cfade433c83a7f3fc19926b0280e6a196ef1b0b5e1b3c1ab426120da53f24e5989f8a7d3dde86ac444901401a6df6407f550d197ff27c91abb5c331250b5a7ce58c1f61fd0d656360df58e6f5b5faac639661aa89402000000bdc1f9592357353a2a3ebcf8cedc72bbf84300000e800000106600001231d67ec689fea9f6de6bd66a21e28d5405232df71deae5bf3ab63c6cb2cac08ad17456979d72b70de13afc2b61d05434161191dcdfb24aaac7ed0275f71eff9f936a559aff4be6301ba99d66bd8e07b1a325c73c7a4da97117f80551a3f0a75da9fcc37c4d2bb43b5a9ef1684446ae0300000000000000000000000000000000000000",
    ))
    .expect("parse masterkey file");
    assert_eq!(file.guid, "ea95eba8-ba00-4e1a-b43f-51ea30171d11");
    assert_eq!(file.master_key.len(), 176);
    assert_eq!(file.backup_key.len(), 144);
    assert_eq!(file.credential_history.len(), 20);
    assert!(file.domain_key.is_empty());
}

#[test]
fn masterkey_decryption_matches_external_oracle() {
    let data = hex(
        "020000000000000000000000650061003900350065006200610038002d0062006100300030002d0034006500310061002d0062003400330066002d00350031006500610033003000310037003100640031003100000000000000000006000000b00000000000000090000000000000001400000000000000000000000000000002000000f42f61ea0c9647bf403819898452089ff84300000e80000010660000e2441dec11a6b6f03ffebba1e71473f78da46a52b12caff7df9925ed6ac89d84050ad15cfe88ec50ece201e5a80eb198909ff8c781510f78859b96cfade433c83a7f3fc19926b0280e6a196ef1b0b5e1b3c1ab426120da53f24e5989f8a7d3dde86ac444901401a6df6407f550d197ff27c91abb5c331250b5a7ce58c1f61fd0d656360df58e6f5b5faac639661aa89402000000bdc1f9592357353a2a3ebcf8cedc72bbf84300000e800000106600001231d67ec689fea9f6de6bd66a21e28d5405232df71deae5bf3ab63c6cb2cac08ad17456979d72b70de13afc2b61d05434161191dcdfb24aaac7ed0275f71eff9f936a559aff4be6301ba99d66bd8e07b1a325c73c7a4da97117f80551a3f0a75da9fcc37c4d2bb43b5a9ef1684446ae0300000000000000000000000000000000000000",
    );
    let prekey = hex("458dc597034d8801fc6fe3b342817caabb81a0cb")
        .try_into()
        .expect("prekey length");
    let recovered = decrypt_master_key_file(&data, &[prekey]).expect("decrypt master key");
    assert_eq!(
        hex::encode(recovered.key),
        "682a9b8923ff4ca7ce0ef7e4cee061f0ff942cd31c7703ec60792740b2e7d0b1b5115d1ff77e10b77e189e0d6e99d5b668190ecd44fa84e82e049f406e2c2a59"
    );

    let parsed = parse_masterkey_file(&data).expect("parse source fixture");
    let mut damaged_primary = parsed.master_key.clone();
    *damaged_primary
        .last_mut()
        .expect("fixture primary section is non-empty") ^= 1;
    let mut backup_fallback_file = data[..128].to_vec();
    backup_fallback_file[96..104].copy_from_slice(&(damaged_primary.len() as u64).to_le_bytes());
    backup_fallback_file[104..112].copy_from_slice(&(parsed.master_key.len() as u64).to_le_bytes());
    backup_fallback_file[112..128].fill(0);
    backup_fallback_file.extend_from_slice(&damaged_primary);
    backup_fallback_file.extend_from_slice(&parsed.master_key);

    let recovered_from_backup = decrypt_master_key_file(&backup_fallback_file, &[prekey])
        .expect("decrypt backup master key after primary integrity failure");
    assert_eq!(recovered_from_backup.guid, recovered.guid);
    assert_eq!(recovered_from_backup.key, recovered.key);
}

#[test]
fn dpapi_blob_decryption_matches_external_oracle() {
    let blob = parse_dpapi_blob(&hex(
        "01000000d08c9ddf0115d1118c7a00c04fc297eb0100000033f19f5ee340be4a8a2e2b4e62bd0cc6000000000200000000001066000000010000200000000d1af96e5e102266fd36d96ac7d1595552e5a4e972463f77e6e227f22d5fc8df000000000e8000000002000020000000834f3c5710c8a7474f7dbcea8ba28ab8e4d4443f50a0c63ff4eba1cce485295f20000000b61d7576c0c6caf3690edb247bde3f7edaa59580e3b4be1265ea78e8c1b8a61d400000001c03ab807147742649b6bdfd1c1344d178bb163842d70abacfd51233af909cb81a677ec05d8db996f587ef5ac410dc189beda756eb0d1b6ee376823e80968538",
    ))
    .expect("parse DPAPI blob");
    let master_key = hex(
        "9828d9873735439e823dbd216205ff88266d28ad685a413970c640d5ee943154bbade31fada673d542c72d707a163bb3d1bceb0c50465b359ae06998481b0ce3",
    );
    let plaintext = blob.decrypt(&master_key).expect("decrypt DPAPI blob");
    assert_eq!(plaintext, b"Some test string");
}
