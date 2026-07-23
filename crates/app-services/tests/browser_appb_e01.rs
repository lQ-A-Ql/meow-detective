//! Pure-E01 offline oracle test for the Chrome 147 App-Bound chain.
//!
//! Every expected value below is an independently documented ground truth for
//! the Liu Yang sample (TBAL LSA secret path, no memory forensics involved).

use app_services::datasource_service::detect_image_filesystem;
use artifacts_windows::browser::{
    parse_chrome_cookies_with_decryptor, parse_chrome_passwords_with_decryptor,
    BrowserDecryptionStatus,
};
use artifacts_windows::dpapi::{
    decrypt_master_key_file, derive_user_prekeys_from_password_sha1, parse_cng_system_key_file,
    ChromiumDecryptor, ChromiumFamily, DecryptedMasterKey,
};
use artifacts_windows::{
    decrypt_lsa_secrets, extract_boot_key, extract_sam_fields, DpapiSystemKeys, TbalSecret,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::{Path, PathBuf};

const EXPECTED_BOOT_KEY: &str = "718189532f7d65193f703fba15323227";
const EXPECTED_LSA_KEY: &str = "5876ac1625a96e1c6aac608940c65f1bf9b3790c0f5669b7ff5f474e912f44ff";
const EXPECTED_TBAL_NT_HASH: &str = "876dfe7bd78730b7b0baaf451414de8e";
const EXPECTED_TBAL_PASSWORD_SHA1: &str = "06499ebb4498d67a10ac7cb0550a11b31b96440a";
const EXPECTED_USER_PREKEY: &str = "f40d41bbabd12d243097a3a649d5498bca684510";
const EXPECTED_USER_MASTER_KEY: &str = "dcb693b8113096e65c404efa462ceb2eb238fec85ebf0606e02c31826cac3c90e6080ce1e33b59abdad7418b05ff655d662ce2d442c6ac3bd09c383f46d1dfc3";
const EXPECTED_SYSTEM_MASTER_KEY_SHA1: &str = "9e3232b3eba7a9f6fa68449a980533cb2b53178d";
const USER_MASTER_KEY_GUID: &str = "be5aeb96-a7e8-4c30-9bf6-3da141dd6608";
const SYSTEM_MASTER_KEY_GUID: &str = "702810de-7de4-4baf-8748-cfdb8031ee08";

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run this test"))
}

fn open_ntfs(fixture: &Path) -> NtfsReader {
    let mut image = E01Reader::open(fixture).expect("open E01");
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
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(fixture).expect("reopen E01"));
    NtfsReader::open(boxed, ntfs.offset).expect("open NTFS")
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

fn find_user_dir(fs: &NtfsReader) -> String {
    fs.list_children("Users")
        .expect("list Users")
        .into_iter()
        .filter(|entry| entry.is_dir)
        .map(|entry| entry.name)
        .find(|name| {
            let path = format!(
                "Users/{name}/AppData/Roaming/Microsoft/Protect/S-1-5-21-3769272433-4215870398-1251094-1002/{USER_MASTER_KEY_GUID}"
            );
            fs.open_file(&path).is_ok()
        })
        .expect("user directory with the target Protect SID directory")
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_chrome147_app_bound_chain_from_pure_e01() {
    let fixture = sample_path();
    assert!(Path::new(&fixture).is_file(), "E01 fixture must be a file");
    let fs = open_ntfs(&fixture);
    let user_dir = find_user_dir(&fs);

    // Registry hives -> BootKey -> LSA key -> secrets.
    let system = read_file(&fs, "Windows/System32/config/SYSTEM").expect("read SYSTEM");
    let security = read_file(&fs, "Windows/System32/config/SECURITY").expect("read SECURITY");
    let sam = read_file(&fs, "Windows/System32/config/SAM").expect("read SAM");
    let boot_key = extract_boot_key(&system).expect("derive SYSTEM boot key");
    assert_eq!(hex::encode(boot_key), EXPECTED_BOOT_KEY);

    let secrets = decrypt_lsa_secrets(&security, &boot_key).expect("decrypt LSA secrets");
    assert_eq!(hex::encode(secrets.lsa_key.as_slice()), EXPECTED_LSA_KEY);

    // TBAL secret -> password-SHA1 -> user prekey.
    let tbal = secrets
        .secrets
        .iter()
        .find_map(|entry| TbalSecret::from_secret(&entry.name, &entry.secret))
        .expect("TBAL primary secret must be present");
    assert_eq!(hex::encode(tbal.nt_hash), EXPECTED_TBAL_NT_HASH);
    assert_eq!(hex::encode(tbal.password_sha1), EXPECTED_TBAL_PASSWORD_SHA1);

    let sam_info = extract_sam_fields(&sam, "offline SAM", Some(boot_key)).expect("parse SAM");
    let user_sid = sam_info
        .users
        .iter()
        .find(|user| {
            user.password_hash
                .as_deref()
                .and_then(|hash| {
                    let (_, nt) = hash.rsplit_once(':')?;
                    hex::decode(nt).ok()?.eq(&tbal.nt_hash).then_some(())
                })
                .is_some()
        })
        .expect("SAM user matching the TBAL NT hash")
        .sid
        .clone();

    let prekeys = derive_user_prekeys_from_password_sha1(&user_sid, &tbal.password_sha1);
    assert_eq!(hex::encode(prekeys[0]), EXPECTED_USER_PREKEY);

    // User master key from the E01 Protect directory.
    let user_mk_path = format!(
        "Users/{user_dir}/AppData/Roaming/Microsoft/Protect/{user_sid}/{USER_MASTER_KEY_GUID}"
    );
    let user_mk_bytes = read_file(&fs, &user_mk_path).expect("read user master-key file");
    let user_master_key =
        decrypt_master_key_file(&user_mk_bytes, &prekeys).expect("decrypt user master key");
    assert_eq!(
        hex::encode(user_master_key.key),
        EXPECTED_USER_MASTER_KEY,
        "TBAL-recovered user master key must match the documented value"
    );

    // SYSTEM master key via the DPAPI_SYSTEM LSA secret.
    let dpapi_system = secrets
        .secret("DPAPI_SYSTEM")
        .expect("DPAPI_SYSTEM secret must be present");
    let system_keys = DpapiSystemKeys::from_secret(dpapi_system).expect("parse DPAPI_SYSTEM");
    let system_mk_path =
        format!("Windows/System32/Microsoft/Protect/S-1-5-18/User/{SYSTEM_MASTER_KEY_GUID}");
    let system_mk_bytes = read_file(&fs, &system_mk_path).expect("read SYSTEM master-key file");
    let system_prekeys = [system_keys.machine_key, system_keys.user_key];
    let system_master_key = decrypt_master_key_file(&system_mk_bytes, &system_prekeys)
        .expect("decrypt SYSTEM master key");
    assert_eq!(
        hex::encode(Sha1::digest(system_master_key.key)),
        EXPECTED_SYSTEM_MASTER_KEY_SHA1
    );

    // Chrome App-Bound inputs from the E01.
    let local_state_path =
        format!("Users/{user_dir}/AppData/Local/Google/Chrome/User Data/Local State");
    let local_state = read_file(&fs, &local_state_path).expect("read Local State");

    let cng_bytes = read_cng_chromekey(&fs).expect("read CNG Google Chromekey1 file");
    let elevation = read_elevation_service(&fs);

    let master_keys: Vec<DecryptedMasterKey> = vec![user_master_key, system_master_key];
    let decryptor = ChromiumDecryptor::from_local_state_with_app_bound(
        &local_state,
        &master_keys,
        ChromiumFamily::Chrome,
        Some(&cng_bytes),
        elevation.as_deref(),
    )
    .expect("build Chromium decryptor");
    assert!(
        decryptor.has_app_bound_key(),
        "App-Bound key must unwrap; error: {:?}",
        decryptor.app_bound_error()
    );
    assert!(
        decryptor.app_bound_bound_to_elevation(),
        "XOR constant must be bound to the extracted elevation_service.exe"
    );

    // End-to-end: the documented v20 login record must decrypt.
    let login_data_path =
        format!("Users/{user_dir}/AppData/Local/Google/Chrome/User Data/Default/Login Data");
    let login_data = read_file(&fs, &login_data_path).expect("read Login Data");
    let passwords = parse_chrome_passwords_with_decryptor(
        &login_data,
        "Chrome",
        Some("Default"),
        Some(&decryptor),
    )
    .expect("parse Login Data");
    let record = passwords
        .iter()
        .find(|password| password.url.contains("jlzb.vip"))
        .expect("documented jlzb.vip login record");
    assert_eq!(record.decryption_status, BrowserDecryptionStatus::Decrypted);
    assert_eq!(record.username, "admin");
    assert_eq!(record.password_preview.as_deref(), Some("admin123"));

    // Edge family chain: raw-key unwrap after the two shared DPAPI layers
    // (no Chrome PostProcessData, no CNG or elevation material required).
    let edge_local_state_path =
        format!("Users/{user_dir}/AppData/Local/Microsoft/Edge/User Data/Local State");
    let edge_local_state = read_file(&fs, &edge_local_state_path).expect("read Edge Local State");
    let edge_decryptor = ChromiumDecryptor::from_local_state_with_app_bound(
        &edge_local_state,
        &master_keys,
        ChromiumFamily::Edge,
        None,
        None,
    )
    .expect("build Edge decryptor");
    assert!(
        edge_decryptor.has_app_bound_key(),
        "Edge App-Bound key must unwrap via the raw-key chain; error: {:?}",
        edge_decryptor.app_bound_error()
    );

    let edge_cookies_path =
        format!("Users/{user_dir}/AppData/Local/Microsoft/Edge/User Data/Default/Network/Cookies");
    if let Ok(edge_cookies) = read_file(&fs, &edge_cookies_path) {
        let cookies = parse_chrome_cookies_with_decryptor(
            &edge_cookies,
            "Edge",
            Some("Default"),
            Some(&edge_decryptor),
        )
        .expect("parse Edge cookies");
        let decrypted = cookies
            .iter()
            .filter(|cookie| cookie.decryption_status == BrowserDecryptionStatus::Decrypted)
            .count();
        eprintln!(
            "Edge cookies: total={} decrypted={decrypted}",
            cookies.len()
        );
        if !cookies.is_empty() {
            assert!(
                decrypted > 0,
                "Edge cookies must decrypt via the Edge chain"
            );
        }
    }
}

fn read_cng_chromekey(fs: &NtfsReader) -> Result<Vec<u8>, String> {
    let dir = "ProgramData/Microsoft/Crypto/SystemKeys";
    for entry in fs
        .list_children(dir)
        .map_err(|error| format!("list {dir}: {error}"))?
        .into_iter()
        .filter(|entry| !entry.is_dir)
    {
        let path = format!("{dir}/{}", entry.name);
        let Ok(bytes) = read_file(fs, &path) else {
            continue;
        };
        let Ok(cng) = parse_cng_system_key_file(&bytes) else {
            continue;
        };
        if cng.description.to_ascii_lowercase().contains("chromekey") {
            return Ok(bytes);
        }
    }
    Err("no Google Chromekey1 CNG system key file found".to_string())
}

fn read_elevation_service(fs: &NtfsReader) -> Option<Vec<u8>> {
    let dir = "Program Files/Google/Chrome/Application";
    for entry in fs.list_children(dir).ok()?.into_iter().filter(|e| e.is_dir) {
        let path = format!("{dir}/{}/elevation_service.exe", entry.name);
        if let Ok(bytes) = read_file(fs, &path) {
            return Some(bytes);
        }
    }
    None
}
