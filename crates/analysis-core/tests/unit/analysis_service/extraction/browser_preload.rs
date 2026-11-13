use super::*;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use rusqlite::Connection;

fn native_source() -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().expect("create native source root");
    let source_dir = root.path().join("case/sources/source-1");
    std::fs::create_dir_all(&source_dir).expect("create source directory");
    let database_path = source_dir.join("source.db");
    let connection =
        persistence_sqlite::open_or_create_source(&database_path).expect("open source database");
    persistence_source::detail::configure_source_identity(&connection, "source-1")
        .expect("configure source identity");
    persistence_source::detail::configure_source_fjall(&connection, &database_path)
        .expect("configure source details");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source-1', 'case-1', 'source', 'e01', '', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("register source");
    (root, connection)
}

fn file_entry(id: &str, path: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".into()),
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        entry_type: EntryType::File,
        size: Some(0),
        ext: path.rsplit_once('.').map(|(_, ext)| ext.into()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(format!("file:{path}")),
        data_source_id: "source-1".to_string(),
        partition_index: Some(2),
        path: path.to_string(),
        size: 4096,
        encrypted: false,
        content_identity: format!("identity:{path}"),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "browser".to_string(),
        parser: "browser.history".to_string(),
        category: "BrowserHistory".to_string(),
    }
}

#[test]
fn dpapi_preload_is_limited_to_chromium_secret_stores() {
    let history = candidate("Users/alice/AppData/Local/Google/Chrome/User Data/Default/History");
    let cookies =
        candidate("Users/alice/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies");

    assert!(browser_roots(&[history]).is_empty());
    assert_eq!(browser_roots(&[cookies]).len(), 1);
}

#[test]
fn encrypted_chromium_secret_store_does_not_trigger_preload_reads() {
    let mut cookies =
        candidate("Users/alice/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies");
    cookies.encrypted = true;

    assert!(browser_roots(&[cookies]).is_empty());
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_preload_builds_app_bound_decryptor_via_service_path() {
    use artifacts_windows::browser::{
        parse_chrome_cookies_with_decryptor, parse_chrome_passwords_with_decryptor,
        BrowserDecryptionStatus,
    };
    use evidence_core::FileSystemReader as _;

    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run this test"));
    let mut image = image_e01::E01Reader::open(&fixture).expect("open E01");
    let probe = evidence_access::datasource_service::detect_image_filesystem(&mut image)
        .expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                evidence_access::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn evidence_core::EvidenceReader> =
        Box::new(image_e01::E01Reader::open(&fixture).expect("reopen E01"));
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).expect("open NTFS");

    let read_ntfs = |path: &str| -> Vec<u8> {
        let mut file = fs
            .open_file(path)
            .unwrap_or_else(|e| panic!("open {path}: {e}"));
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).expect("read file");
        bytes
    };

    let (_source_root, conn) = native_source();
    let ntfs_paths = [
        "Windows/System32/config/SYSTEM",
        "Windows/System32/config/SAM",
        "Windows/System32/config/SECURITY",
        "Users/刘洋/AppData/Roaming/Microsoft/Protect/S-1-5-21-3769272433-4215870398-1251094-1002/be5aeb96-a7e8-4c30-9bf6-3da141dd6608",
        "Windows/System32/Microsoft/Protect/S-1-5-18/User/702810de-7de4-4baf-8748-cfdb8031ee08",
        "Users/刘洋/AppData/Local/Google/Chrome/User Data/Local State",
        "Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies",
        "Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Login Data",
        "ProgramData/Microsoft/Crypto/SystemKeys/7096db7aeb75c0d3497ecd56d355a695_32a3bf44-0767-4a67-8288-138d1cc50a88",
        "Program Files/Google/Chrome/Application/147.0.7727.102/elevation_service.exe",
    ];
    let files = ntfs_paths
        .iter()
        .enumerate()
        .map(|(index, path)| file_entry(&format!("file:{index}"), &format!("[P2]/{path}")))
        .collect::<Vec<_>>();
    persistence_source::repositories::file_repo::FileRepo::new(&conn)
        .insert_batch(&files)
        .expect("insert native file entries");

    let candidates = [
        "[P2]/Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies",
        "[P2]/Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Login Data",
    ]
    .into_iter()
    .map(candidate)
    .collect::<Vec<_>>();

    let mut reader = |candidate: &EvidenceCandidate, _limit: usize| {
        let path = candidate
            .path
            .strip_prefix("[P2]/")
            .unwrap_or(&candidate.path);
        Ok(CandidateSource::Bytes(read_ntfs(path)))
    };
    let cancel_token = AtomicBool::new(false);
    let catalog = preparation_catalog(&conn);
    let context = prepare_browser_preload(
        &catalog,
        &super::super::registry_preload::RegistryPreloadContext::default(),
        &candidates,
        &cancel_token,
        None,
        &mut reader,
    )
    .expect("browser preload");
    eprintln!("preload warnings: {:?}", context.warnings);

    let decryptor = context
        .decryptor_for(&candidates[0])
        .expect("decryptor for the Chrome profile root");
    assert!(
        decryptor.has_app_bound_key(),
        "App-Bound key must unwrap via the service path; error: {:?}",
        decryptor.app_bound_error()
    );

    fn strip(path: &str) -> &str {
        path.strip_prefix("[P2]/").unwrap_or(path)
    }
    let cookies = parse_chrome_cookies_with_decryptor(
        &read_ntfs(strip(&candidates[0].path)),
        "Chrome",
        Some("Default"),
        Some(decryptor),
    )
    .expect("parse cookies");
    let decrypted_cookies = cookies
        .iter()
        .filter(|cookie| cookie.decryption_status == BrowserDecryptionStatus::Decrypted)
        .count();
    eprintln!(
        "cookies: total={} decrypted={decrypted_cookies}",
        cookies.len()
    );
    assert!(decrypted_cookies > 0, "v20 cookies must decrypt");

    let passwords = parse_chrome_passwords_with_decryptor(
        &read_ntfs(strip(&candidates[1].path)),
        "Chrome",
        Some("Default"),
        Some(decryptor),
    )
    .expect("parse passwords");
    let record = passwords
        .iter()
        .find(|password| password.url.contains("jlzb.vip"))
        .expect("documented jlzb.vip login record");
    assert_eq!(record.decryption_status, BrowserDecryptionStatus::Decrypted);
    assert_eq!(record.password_preview.as_deref(), Some("admin123"));
}

#[test]
fn locate_by_suffix_tolerates_all_stored_path_forms() {
    let (_source_root, conn) = native_source();
    let files = [
        "Windows/System32/config/SYSTEM",
        "[P3]/Windows/System32/config/SAM",
        "/Windows/System32/config/SECURITY",
        "ConfigWindows/System32/config/SOFTWARE",
    ]
    .iter()
    .enumerate()
    .map(|(index, path)| file_entry(&format!("file:{index}"), path))
    .collect::<Vec<_>>();
    persistence_source::repositories::file_repo::FileRepo::new(&conn)
        .insert_batch(&files)
        .expect("insert native file entries");

    let catalog = preparation_catalog(&conn);
    assert!(
        catalog
            .suffix("source-1", "/windows/system32/config/system")
            .is_some(),
        "bare relative path without partition prefix must be found"
    );
    assert!(
        catalog
            .suffix("source-1", "/windows/system32/config/sam")
            .is_some(),
        "partition-prefixed path must still be found"
    );
    assert!(
        catalog
            .suffix("source-1", "/windows/system32/config/security")
            .is_some(),
        "leading-slash path must be found"
    );
    assert!(
        catalog
            .suffix("source-1", "/windows/system32/config/software")
            .is_none(),
        "segment-boundary mismatches must stay rejected"
    );
}

#[test]
fn elevation_service_selection_is_bound_to_the_chromium_family() {
    let (_source_root, conn) = native_source();
    let files = [
        (
            "edge",
            "Program Files/Microsoft/Edge/Application/120/elevation_service.exe",
        ),
        (
            "chrome",
            "Program Files/Google/Chrome/Application/147/elevation_service.exe",
        ),
    ]
    .into_iter()
    .map(|(id, path)| file_entry(id, path))
    .collect::<Vec<_>>();
    persistence_source::repositories::file_repo::FileRepo::new(&conn)
        .insert_batch(&files)
        .expect("insert native file entries");
    let cancel = AtomicBool::new(false);
    let catalog = preparation_catalog(&conn);
    let mut reader = |candidate: &EvidenceCandidate, _limit: usize| {
        Ok(CandidateSource::Bytes(
            candidate.file_id.0.as_bytes().to_vec(),
        ))
    };

    let chrome = read_elevation_service(
        &catalog,
        "source-1",
        ChromiumFamily::Chrome,
        &cancel,
        &mut reader,
    )
    .expect("Chrome elevation service");
    let edge = read_elevation_service(
        &catalog,
        "source-1",
        ChromiumFamily::Edge,
        &cancel,
        &mut reader,
    )
    .expect("Edge elevation service");

    assert_eq!(chrome.as_deref(), Some(b"chrome".as_slice()));
    assert_eq!(edge.as_deref(), Some(b"edge".as_slice()));
}

fn preparation_catalog(conn: &rusqlite::Connection) -> PreparationCatalog {
    crate::analysis_service::candidates::evidence_candidates_for_categories_with_preparation(
        conn,
        &["Registry", "BrowserHistory"],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap()
    .2
}
