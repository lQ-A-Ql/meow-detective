use app_services::datasource_service::detect_image_filesystem;
use artifacts_windows::browser::{
    parse_chrome_cookies_with_decryptor, parse_chrome_passwords_with_decryptor,
    BrowserDecryptionStatus,
};
use artifacts_windows::dpapi::{
    decrypt_master_key_file, derive_user_prekeys, parse_masterkey_file, ChromiumDecryptor,
    DecryptedMasterKey,
};
use artifacts_windows::{extract_boot_key, extract_sam_fields};
use evidence_core::{EvidenceReader, FileSystemReader};
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run this test"))
}

fn read_file(fs: &NtfsReader, path: &str) -> Result<Vec<u8>, String> {
    let mut reader = fs
        .open_file(path)
        .map_err(|error| format!("open {path}: {error}"))?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {path}: {error}"))?;
    Ok(bytes)
}

fn nt_hash_from_sam(value: &str) -> Option<[u8; 16]> {
    let hex_value = value.rsplit_once(':').map(|(_, nt)| nt).unwrap_or(value);
    let bytes = hex::decode(hex_value).ok()?;
    bytes.try_into().ok()
}

fn collect_master_keys(
    fs: &NtfsReader,
    users_root: &str,
    prekeys_by_sid: &HashMap<String, Vec<[u8; 20]>>,
) -> Vec<DecryptedMasterKey> {
    let mut recovered = HashMap::<String, DecryptedMasterKey>::new();
    let Ok(user_dirs) = fs.list_children(users_root) else {
        eprintln!("DPAPI diagnostics: Users directory could not be listed");
        return Vec::new();
    };
    let mut sid_dir_count = 0usize;
    let mut guid_file_count = 0usize;
    let mut read_failure_count = 0usize;
    let mut decrypt_failure_count = 0usize;
    for user_dir in user_dirs.into_iter().filter(|entry| entry.is_dir) {
        let protect_root = format!(
            "{}/{}/AppData/Roaming/Microsoft/Protect",
            users_root.trim_end_matches('/'),
            user_dir.name
        );
        let Ok(sid_dirs) = fs.list_children(&protect_root) else {
            continue;
        };
        for sid_dir in sid_dirs.into_iter().filter(|entry| entry.is_dir) {
            sid_dir_count += 1;
            let sid = sid_dir.name.to_ascii_lowercase();
            let Some(prekeys) = prekeys_by_sid.get(&sid) else {
                continue;
            };
            let sid_path = format!("{protect_root}/{}", sid_dir.name);
            let Ok(files) = fs.list_children(&sid_path) else {
                continue;
            };
            for file in files.into_iter().filter(|entry| !entry.is_dir) {
                if Uuid::parse_str(&file.name).is_err() {
                    continue;
                }
                guid_file_count += 1;
                let path = format!("{sid_path}/{}", file.name);
                let Ok(bytes) = read_file(fs, &path) else {
                    read_failure_count += 1;
                    continue;
                };
                let master_key = match decrypt_master_key_file(&bytes, prekeys) {
                    Ok(master_key) => master_key,
                    Err(error) => {
                        decrypt_failure_count += 1;
                        if let Ok(file) = parse_masterkey_file(&bytes) {
                            let section = &file.master_key;
                            let rounds = section
                                .get(20..24)
                                .and_then(|value| value.try_into().ok())
                                .map(u32::from_le_bytes);
                            let hash = section
                                .get(24..28)
                                .and_then(|value| value.try_into().ok())
                                .map(u32::from_le_bytes);
                            let cipher = section
                                .get(28..32)
                                .and_then(|value| value.try_into().ok())
                                .map(u32::from_le_bytes);
                            eprintln!(
                                "DPAPI diagnostics: sid={} guid={} version={} master_len={} rounds={rounds:?} hash={hash:?} cipher={cipher:?}",
                                sid_dir.name,
                                file.guid,
                                file.version,
                                file.master_key.len()
                            );
                        }
                        eprintln!(
                            "DPAPI diagnostics: master-key file bytes={} prekeys={} error={error}",
                            bytes.len(),
                            prekeys.len()
                        );
                        continue;
                    }
                };
                recovered
                    .entry(master_key.guid.to_ascii_lowercase())
                    .or_insert(master_key);
            }
        }
    }
    eprintln!(
        "DPAPI diagnostics: sid_dirs={sid_dir_count} guid_files={guid_file_count} \
         read_failures={read_failure_count} decrypt_failures={decrypt_failure_count} \
         recovered={}",
        recovered.len()
    );
    recovered.into_values().collect()
}

fn assert_decrypted(
    statuses: impl IntoIterator<Item = BrowserDecryptionStatus>,
    label: &str,
) -> usize {
    let counts = statuses
        .into_iter()
        .fold(HashMap::new(), |mut counts, status| {
            *counts.entry(status).or_insert(0usize) += 1;
            counts
        });
    let decrypted = counts
        .get(&BrowserDecryptionStatus::Decrypted)
        .copied()
        .unwrap_or(0);
    eprintln!("{label}: statuses={counts:?}");
    decrypted
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_chromium_cookies_and_passwords_decrypt_offline() {
    let fixture = sample_path();
    assert!(Path::new(&fixture).is_file(), "E01 fixture must be a file");

    let mut image = E01Reader::open(&fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang E01 must expose an NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");

    let system = read_file(&fs, "Windows/System32/config/SYSTEM").expect("read SYSTEM");
    let sam = read_file(&fs, "Windows/System32/config/SAM").expect("read SAM");
    let boot_key = extract_boot_key(&system).expect("derive SYSTEM boot key");
    let sam_info = extract_sam_fields(&sam, "offline SAM", Some(boot_key)).expect("parse SAM");
    let prekeys_by_sid = sam_info
        .users
        .iter()
        .filter_map(|user| {
            let hash = user.password_hash.as_deref()?;
            let nt_hash = nt_hash_from_sam(hash)?;
            Some((
                user.sid.to_ascii_lowercase(),
                derive_user_prekeys(&user.sid, &nt_hash),
            ))
        })
        .collect::<HashMap<_, _>>();
    eprintln!(
        "DPAPI diagnostics: SAM users={} prekey_sets={} prekeys={:?}",
        sam_info.users.len(),
        prekeys_by_sid.len(),
        prekeys_by_sid.values().map(Vec::len).collect::<Vec<_>>()
    );
    if let Some(expected) = std::env::var_os("FORENSICS_EXPECTED_NT_HASH") {
        let expected = expected.to_string_lossy().to_ascii_lowercase();
        for user in &sam_info.users {
            let actual = user
                .password_hash
                .as_deref()
                .and_then(|value| value.rsplit_once(':').map(|(_, hash)| hash));
            eprintln!(
                "DPAPI diagnostics: user={} sid={} rid={} expected_nt_match={}",
                user.username,
                user.sid,
                user.rid,
                actual.is_some_and(|hash| hash.eq_ignore_ascii_case(&expected))
            );
        }
    }
    assert!(
        !prekeys_by_sid.is_empty(),
        "SAM must yield DPAPI user pre-keys"
    );

    let master_keys = collect_master_keys(&fs, "Users", &prekeys_by_sid);
    assert!(
        !master_keys.is_empty(),
        "Protect must yield a DPAPI master key"
    );

    let mut decryptors = Vec::new();
    for user in fs
        .list_children("Users")
        .expect("list Users")
        .into_iter()
        .filter(|entry| entry.is_dir)
    {
        for (browser, local_state) in [
            (
                "Chrome",
                format!(
                    "Users/{}/AppData/Local/Google/Chrome/User Data/Local State",
                    user.name
                ),
            ),
            (
                "Edge",
                format!(
                    "Users/{}/AppData/Local/Microsoft/Edge/User Data/Local State",
                    user.name
                ),
            ),
        ] {
            let Ok(bytes) = read_file(&fs, &local_state) else {
                continue;
            };
            if let Ok(decryptor) = ChromiumDecryptor::from_local_state(&bytes, &master_keys) {
                decryptors.push((browser, user.name.clone(), decryptor));
            }
        }
    }
    assert!(
        !decryptors.is_empty(),
        "at least one Chromium Local State must decrypt"
    );

    let mut cookie_statuses = Vec::new();
    let mut password_statuses = Vec::new();
    for (browser, user, decryptor) in &decryptors {
        let (cookies_path, passwords_path) = if *browser == "Chrome" {
            (
                format!(
                    "Users/{user}/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies"
                ),
                format!("Users/{user}/AppData/Local/Google/Chrome/User Data/Default/Login Data"),
            )
        } else {
            (
                format!(
                    "Users/{user}/AppData/Local/Microsoft/Edge/User Data/Default/Network/Cookies"
                ),
                format!("Users/{user}/AppData/Local/Microsoft/Edge/User Data/Default/Login Data"),
            )
        };
        if let Ok(bytes) = read_file(&fs, &cookies_path) {
            let cookies = parse_chrome_cookies_with_decryptor(
                &bytes,
                browser,
                Some("Default"),
                Some(decryptor),
            )
            .expect("parse Chromium Cookies");
            cookie_statuses.extend(cookies.into_iter().map(|cookie| cookie.decryption_status));
        }
        if let Ok(bytes) = read_file(&fs, &passwords_path) {
            let passwords = parse_chrome_passwords_with_decryptor(
                &bytes,
                browser,
                Some("Default"),
                Some(decryptor),
            )
            .expect("parse Chromium Login Data");
            password_statuses.extend(
                passwords
                    .into_iter()
                    .map(|password| password.decryption_status),
            );
        }
    }

    assert!(
        !cookie_statuses.is_empty(),
        "sample must contain Chromium cookies"
    );
    assert!(
        !password_statuses.is_empty(),
        "sample must contain Chromium passwords"
    );
    assert!(
        assert_decrypted(cookie_statuses, "cookies") > 0,
        "at least one Chromium cookie must decrypt"
    );
    assert!(
        assert_decrypted(password_statuses, "passwords") > 0,
        "at least one Chromium password must decrypt"
    );
}
