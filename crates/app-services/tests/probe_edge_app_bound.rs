//! Diagnostic probe: inspect Edge's app_bound_encrypted_key structure in the
//! liuyang E01 (why the Chrome App-Bound chain rejects it with
//! "unsupported DPAPI version 106").

use app_services::datasource_service::detect_image_filesystem;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn probe_edge_app_bound_structure() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
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
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");

    let path = "Users/刘洋/AppData/Local/Microsoft/Edge/User Data/Local State";
    let mut file = fs.open_file(path).expect("open Edge Local State");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("read Edge Local State");
    let root: serde_json::Value = serde_json::from_slice(&bytes).expect("parse Local State");
    let os_crypt = root.get("os_crypt").expect("os_crypt");
    for key in ["encrypted_key", "app_bound_encrypted_key"] {
        eprintln!("os_crypt.{key} present: {}", os_crypt.get(key).is_some());
    }
    let encoded = os_crypt
        .get("app_bound_encrypted_key")
        .and_then(serde_json::Value::as_str)
        .expect("app_bound_encrypted_key");
    let wrapped = STANDARD.decode(encoded).expect("base64 decode");
    eprintln!("decoded length: {}", wrapped.len());
    eprintln!("first 4 bytes: {:?}", &wrapped[..4]);
    let after_prefix = &wrapped[4..];
    let version = u32::from_le_bytes(after_prefix[..4].try_into().unwrap());
    eprintln!("u32 after APPB prefix: {version} ({version:#x})");
    eprintln!(
        "first 48 bytes after prefix: {}",
        hex::encode(&after_prefix[..48])
    );

    if let Ok(blob) = artifacts_windows::dpapi::parse_dpapi_blob(after_prefix) {
        eprintln!("outer blob master key guid: {}", blob.master_key_guid);
    } else {
        eprintln!("parse_dpapi_blob failed on Edge outer blob");
    }

    // Recover the SYSTEM master key and inspect the outer-layer plaintext.
    let read_ntfs = |path: &str| -> Vec<u8> {
        let mut file = fs
            .open_file(path)
            .unwrap_or_else(|e| panic!("open {path}: {e}"));
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("read file");
        bytes
    };
    let system = read_ntfs("Windows/System32/config/SYSTEM");
    let security = read_ntfs("Windows/System32/config/SECURITY");
    let boot_key = artifacts_windows::extract_boot_key(&system).expect("boot key");
    let secrets =
        artifacts_windows::decrypt_lsa_secrets(&security, &boot_key).expect("lsa secrets");
    let keys = artifacts_windows::DpapiSystemKeys::from_secret(
        secrets.secret("DPAPI_SYSTEM").expect("DPAPI_SYSTEM"),
    )
    .expect("parse DPAPI_SYSTEM");
    let prekeys = [keys.machine_key, keys.user_key];
    let mk_bytes = read_ntfs(
        "Windows/System32/Microsoft/Protect/S-1-5-18/User/702810de-7de4-4baf-8748-cfdb8031ee08",
    );
    let system_mk = artifacts_windows::dpapi::decrypt_master_key_file(&mk_bytes, &prekeys)
        .expect("decrypt SYSTEM master key");

    let outer = artifacts_windows::dpapi::parse_dpapi_blob(after_prefix).expect("outer blob");
    let outer_plaintext = outer.decrypt(&system_mk.key).expect("decrypt outer layer");
    eprintln!("outer plaintext length: {}", outer_plaintext.len());
    let dump_len = outer_plaintext.len().min(96);
    eprintln!(
        "outer plaintext prefix: {}",
        hex::encode(&outer_plaintext[..dump_len])
    );
    match artifacts_windows::dpapi::parse_dpapi_blob(&outer_plaintext) {
        Ok(inner) => eprintln!("inner blob master key guid: {}", inner.master_key_guid),
        Err(error) => eprintln!("inner is not a DPAPI blob: {error}"),
    }

    // Recover the user master key via the TBAL path and dump the inner plaintext.
    let sam = read_ntfs("Windows/System32/config/SAM");
    let sam_info = artifacts_windows::extract_sam_fields(&sam, "offline SAM", Some(boot_key))
        .expect("parse SAM");
    let tbal = secrets
        .secrets
        .iter()
        .find_map(|entry| artifacts_windows::TbalSecret::from_secret(&entry.name, &entry.secret))
        .expect("TBAL secret");
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
        .expect("SAM user matching TBAL")
        .sid
        .clone();
    let user_prekeys = artifacts_windows::dpapi::derive_user_prekeys_from_password_sha1(
        &user_sid,
        &tbal.password_sha1,
    );
    let user_mk_bytes = read_ntfs(&format!(
        "Users/刘洋/AppData/Roaming/Microsoft/Protect/{user_sid}/be5aeb96-a7e8-4c30-9bf6-3da141dd6608"
    ));
    let user_mk = artifacts_windows::dpapi::decrypt_master_key_file(&user_mk_bytes, &user_prekeys)
        .expect("decrypt user master key");

    let inner = artifacts_windows::dpapi::parse_dpapi_blob(&outer_plaintext).expect("inner blob");
    let inner_plaintext = inner.decrypt(&user_mk.key).expect("decrypt inner layer");
    eprintln!("inner plaintext length: {}", inner_plaintext.len());
    eprintln!("inner plaintext hex: {}", hex::encode(&inner_plaintext));
    match artifacts_windows::dpapi::parse_chrome_key_blob(&inner_plaintext) {
        Ok(blob) => eprintln!(
            "chrome key blob: path={} content_len={} flag={:#x}",
            blob.validation_path,
            blob.content.len(),
            blob.content.first().copied().unwrap_or(0)
        ),
        Err(error) => eprintln!("not a Chrome key blob: {error}"),
    }
}
