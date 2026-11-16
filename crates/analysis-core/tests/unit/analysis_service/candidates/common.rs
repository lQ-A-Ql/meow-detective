use super::*;
crate::native_source_test_support!();

#[test]
fn normalization_removes_partition_and_lvm_roots_without_losing_linux_root() {
    let cases = [
        ("Partition 2 (XFS) - cl/root/etc/passwd", "/etc/passwd"),
        (
            "[P2]/cl/root/var/log/auth.log.1.gz",
            "/var/log/auth.log.1.gz",
        ),
        (
            "cl/root/home/alice/.bash_history",
            "/home/alice/.bash_history",
        ),
        ("cl/root/root/.bash_history", "/root/.bash_history"),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_evidence_path(input), expected);
    }
}

#[test]
fn normalization_preserves_windows_paths_after_partition_marker_removal() {
    assert_eq!(
        normalize_evidence_path(r"[P0]\Windows\System32\config\SYSTEM"),
        "/windows/system32/config/system"
    );
}

#[test]
fn evidence_relative_paths_preserve_linux_case_after_removing_catalog_roots() {
    for path in [
        "Partition 4 (EXT4) - ubuntu-vg/ubuntu-lv/etc/systemd/system/Worker.service",
        "[P4]/ubuntu-vg/ubuntu-lv/etc/systemd/system/Worker.service",
        "/etc/systemd/system/Worker.service",
    ] {
        assert_eq!(
            evidence_relative_path(path),
            "/etc/systemd/system/Worker.service"
        );
    }
}

#[test]
fn linux_category_selection_excludes_windows_definitions_before_scan() {
    let definitions = selected_evidence_category_defs(&["LinuxArtifacts"]);
    let categories = definitions
        .iter()
        .map(|definition| definition.category)
        .collect::<Vec<_>>();

    assert_eq!(categories, vec!["LinuxArtifacts"]);
    assert!(!categories.contains(&"Registry"));
    assert!(!categories.contains(&"EventLogs"));
    assert!(!categories.contains(&"BrowserHistory"));
}

fn image_record(id: usize, path: &str, size: u64) -> FileEntryRecord {
    FileEntryRecord {
        id: format!("image-{id:05}"),
        parent_id: None,
        data_source_id: "source".into(),
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        entry_type: CatalogEntryType::File,
        size: Some(size),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(0),
        unix_mode: None,
    }
}

fn image_fixture(records: &[FileEntryRecord]) -> test_support::NativeSourceFixture {
    let fixture = test_support::NativeSourceFixture::new("source", "case");
    let store = persistence_source::detail::open_store_for_write(&fixture)
        .unwrap()
        .unwrap();
    persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone())
        .upsert_batch(records, persistence_source::fjall::PersistMode::SyncAll)
        .unwrap();
    fixture
}

#[test]
fn magic_discovery_ignores_extensions_and_skips_ineligible_entries() {
    let mut directory = image_record(7, "/evidence/album.bmp", 0);
    directory.entry_type = CatalogEntryType::Directory;
    let mut records = vec![
        image_record(1, "/evidence/no-extension", 64),
        image_record(2, "/evidence/notes.txt", 64),
        image_record(3, "/evidence/photo.jpg", 64),
        image_record(4, "/evidence/secret.png", 64),
        image_record(5, "/evidence/empty.png", 0),
        directory,
    ];
    records[3].encrypted = Some(true);
    let fixture = image_fixture(&records);

    let mut probed = Vec::new();
    let (candidates, _, _) = evidence_candidates_for_categories_with_preparation_and_probe(
        &fixture,
        &["ImageMetadata"],
        &AtomicBool::new(false),
        &mut |entry| {
            probed.push(entry.path.clone());
            Ok(match entry.path.as_str() {
                "/evidence/no-extension" => Some("JPEG"),
                "/evidence/notes.txt" => Some("PNG"),
                _ => None,
            })
        },
        |_| {},
    )
    .unwrap();

    let mut paths = candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, vec!["/evidence/no-extension", "/evidence/notes.txt"]);
    assert!(candidates.iter().all(|candidate| {
        candidate.category == "ImageMetadata" && candidate.parser == "image.metadata.container"
    }));
    assert!(!probed.iter().any(|path| path == "/evidence/secret.png"));
    assert!(!probed.iter().any(|path| path == "/evidence/empty.png"));
    assert!(!probed.iter().any(|path| path == "/evidence/album.bmp"));
    assert!(probed.iter().any(|path| path == "/evidence/photo.jpg"));
}

#[test]
fn magic_probe_errors_do_not_abort_other_categories() {
    let records = vec![
        image_record(1, "[P0]/Windows/System32/config/SYSTEM", 64),
        image_record(2, "/evidence/broken.png", 64),
        image_record(3, "/evidence/ok.png", 64),
    ];
    let fixture = image_fixture(&records);

    let mut probed = Vec::new();
    let (candidates, _, _) = evidence_candidates_for_categories_with_preparation_and_probe(
        &fixture,
        &["Registry", "ImageMetadata"],
        &AtomicBool::new(false),
        &mut |entry| {
            probed.push(entry.path.clone());
            if entry.path.contains("broken") {
                return Err("simulated header read failure".to_string());
            }
            Ok(Some("PNG"))
        },
        |_| {},
    )
    .unwrap();

    assert!(candidates
        .iter()
        .any(|candidate| candidate.category == "Registry"));
    let images = candidates
        .iter()
        .filter(|candidate| candidate.category == "ImageMetadata")
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(images, vec!["/evidence/ok.png"]);
    assert_eq!(probed, vec!["/evidence/broken.png", "/evidence/ok.png"]);
}

#[test]
fn discovery_without_image_metadata_never_probes() {
    let records = vec![image_record(1, "/evidence/plain.png", 64)];
    let fixture = image_fixture(&records);

    let mut probed = 0;
    let (candidates, _, _) = evidence_candidates_for_categories_with_preparation_and_probe(
        &fixture,
        &["Registry"],
        &AtomicBool::new(false),
        &mut |_| {
            probed += 1;
            Ok(Some("PNG"))
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(probed, 0);
    assert!(candidates.is_empty());
}

#[test]
fn summary_discovery_never_reports_image_metadata_candidates() {
    let records = vec![image_record(1, "/evidence/no-extension", 64)];
    let fixture = image_fixture(&records);

    let groups = discover_evidence_candidates(&fixture).unwrap();

    assert!(groups
        .get("ImageMetadata")
        .is_none_or(|group| group.is_empty()));
}
