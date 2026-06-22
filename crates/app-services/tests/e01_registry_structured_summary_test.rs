use artifacts_windows::{
    extract_boot_key, extract_installed_software, extract_network_adapters_from_system_hive,
    extract_ntuser_fields, extract_sam_fields,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use std::io::Read;

fn sample_path() -> std::path::PathBuf {
    testing::fixtures::local_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 registry summary test")
    })
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_registry_network_adapters_and_default_browser() {
    let probe = {
        let mut reader = E01Reader::open(&sample_path()).unwrap();
        app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap()
    };

    // Find the NTFS partition that actually contains the Windows directory.
    let mut fs = None;
    for candidate in &probe.candidates {
        if !matches!(
            candidate.kind,
            app_services::datasource_service::ImageFilesystemKind::Ntfs
        ) {
            continue;
        }
        let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&sample_path()).unwrap());
        let candidate_fs = NtfsReader::open(boxed, candidate.offset).unwrap();
        if candidate_fs
            .list_children("Windows/System32/config")
            .map(|c| !c.is_empty())
            .unwrap_or(false)
        {
            eprintln!("Using NTFS candidate at offset {:#x}", candidate.offset);
            fs = Some(candidate_fs);
            break;
        }
    }
    let fs = fs.expect("No NTFS partition contains Windows/System32/config");

    eprintln!("Windows/System32/config children:");
    for child in fs.list_children("Windows/System32/config").unwrap().iter() {
        eprintln!(
            "  {} {} size={}",
            if child.is_dir { "D" } else { "F" },
            child.name,
            child.size
        );
    }

    // Read SYSTEM hive and extract network adapters.
    let system_bytes = read_file(&fs, "Windows/System32/config/SYSTEM");
    let adapters =
        extract_network_adapters_from_system_hive(&system_bytes, "Windows/System32/config/SYSTEM")
            .expect("SYSTEM hive parse failed");
    eprintln!("Network adapters: {}", adapters.len());
    for adapter in &adapters {
        eprintln!(
            "  name={:?} ip={:?} gateway={:?} mac={:?} dhcp={:?}",
            adapter.name,
            adapter.ip_address,
            adapter.gateway,
            adapter.mac_address,
            adapter.dhcp_enabled
        );
    }
    assert!(
        !adapters.is_empty(),
        "Expected at least one network adapter"
    );
    assert!(
        adapters
            .iter()
            .any(|a| a.ip_address.is_some() || a.gateway.is_some()),
        "Expected at least one adapter with IP or gateway"
    );

    // Read an NTUSER.DAT and extract default browser if present.
    let ntuser_paths = ["Users/Administrator/NTUSER.DAT", "Users/Default/NTUSER.DAT"];
    let mut browser_found = false;
    for path in &ntuser_paths {
        let bytes = read_file(&fs, path);
        if !bytes.starts_with(b"regf") {
            continue;
        }
        let info = extract_ntuser_fields(&bytes, path).expect("NTUSER parse failed");
        if let Some(browser) = info.default_browser {
            eprintln!("Default browser from {}: {}", path, browser);
            browser_found = true;
            break;
        }
    }
    // Default browser is optional; just record whether it was found.
    eprintln!("Default browser found: {}", browser_found);

    // AweSun .hive files must NOT be treated as registry hives.
    let awe_sun_dir = "Users/Administrator/AppData/Roaming/Oray/AweSun";
    match fs.list_children(awe_sun_dir) {
        Ok(children) => {
            let hive_files: Vec<_> = children
                .into_iter()
                .filter(|c| !c.is_dir && c.name.to_ascii_lowercase().ends_with(".hive"))
                .collect();
            eprintln!("AweSun .hive files: {}", hive_files.len());
            for file in &hive_files {
                let bytes = read_file(&fs, &file.path);
                assert!(
                    !bytes.starts_with(b"regf"),
                    "{} should not be a valid regf hive",
                    file.path
                );
            }
        }
        Err(err) => eprintln!("Could not list {}: {}", awe_sun_dir, err),
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_registry_sam_users_and_installed_software() {
    let probe = {
        let mut reader = E01Reader::open(&sample_path()).unwrap();
        app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap()
    };

    let mut fs = None;
    for candidate in &probe.candidates {
        if !matches!(
            candidate.kind,
            app_services::datasource_service::ImageFilesystemKind::Ntfs
        ) {
            continue;
        }
        let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&sample_path()).unwrap());
        let candidate_fs = NtfsReader::open(boxed, candidate.offset).unwrap();
        if candidate_fs
            .list_children("Windows/System32/config")
            .map(|c| !c.is_empty())
            .unwrap_or(false)
        {
            fs = Some(candidate_fs);
            break;
        }
    }
    let fs = fs.expect("No NTFS partition contains Windows/System32/config");

    let system_bytes = read_file(&fs, "Windows/System32/config/SYSTEM");
    let boot_key = extract_boot_key(&system_bytes);
    let sam_bytes = read_file(&fs, "Windows/System32/config/SAM");
    let sam = extract_sam_fields(&sam_bytes, "Windows/System32/config/SAM", boot_key)
        .expect("SAM hive parse failed");
    eprintln!("SAM users: {}", sam.users.len());
    for user in &sam.users {
        eprintln!(
            "  {} rid={} sid={} groups={:?} login_count={} last_login={:?} status={}/{} hash={}",
            user.username,
            user.rid,
            user.sid,
            user.group_memberships,
            user.login_count,
            user.last_login,
            if user.account_disabled {
                "disabled"
            } else {
                "enabled"
            },
            if user.account_locked {
                "locked"
            } else {
                "unlocked"
            },
            if user.password_hash.is_some() {
                "<redacted>"
            } else {
                "-"
            }
        );
    }
    assert!(!sam.users.is_empty(), "Expected at least one SAM user");
    let admin = sam
        .users
        .iter()
        .find(|u| u.username.eq_ignore_ascii_case("Administrator"));
    if let Some(admin) = admin {
        assert!(
            !admin.group_memberships.is_empty(),
            "Administrator should belong to at least one group"
        );
        assert!(
            admin.sid.starts_with("S-1-5-21-"),
            "Administrator should have a valid machine SID, got {}",
            admin.sid
        );
        assert!(
            admin.password_hash.is_some(),
            "Administrator should have a decrypted password hash when SYSTEM BootKey is available"
        );
        let hash = admin.password_hash.as_deref().unwrap();
        assert!(
            hash.len() == 65 && hash.as_bytes()[32] == b':',
            "Password hash should be lm:nt format, got {}",
            hash
        );
    }

    let software_bytes = read_file(&fs, "Windows/System32/config/SOFTWARE");
    let software = extract_installed_software(&software_bytes, "Windows/System32/config/SOFTWARE")
        .expect("SOFTWARE hive parse failed");
    eprintln!("Installed software entries: {}", software.len());
    for entry in software.iter().take(10) {
        eprintln!(
            "  {} version={:?} publisher={:?} size={:?}",
            entry.display_name, entry.version, entry.publisher, entry.estimated_size_kb
        );
    }
    assert!(
        software.len() > 1,
        "Expected multiple installed software entries"
    );
}

fn read_file(fs: &NtfsReader, path: &str) -> Vec<u8> {
    let mut reader = fs
        .open_file(path)
        .unwrap_or_else(|e| panic!("open {}: {}", path, e));
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    bytes
}
