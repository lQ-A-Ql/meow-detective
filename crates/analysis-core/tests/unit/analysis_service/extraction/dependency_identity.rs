use super::*;
use domain::FileEntryId;
use persistence_source::fjall::FileEntryRecord;
use std::{fs, path::Path};

fn workspace() -> tempfile::TempDir {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/dependency-tests");
    fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}

fn record(id: &str, path: &str, partition: usize) -> FileEntryRecord {
    FileEntryRecord {
        id: id.into(),
        parent_id: None,
        data_source_id: "source".into(),
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        entry_type: CatalogEntryType::File,
        size: Some(4),
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
        partition_index: Some(partition as u32),
        unix_mode: None,
    }
}

fn candidate(category: &str, path: &str, partition: usize) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("primary".into()),
        data_source_id: "source".into(),
        partition_index: Some(partition),
        path: path.into(),
        size: 4,
        encrypted: false,
        content_identity: "primary-input".into(),
        companions: Vec::new(),
        evidence_kind: "file".into(),
        parser: "analysis".into(),
        category: category.into(),
        modified_at: None,
    }
}

fn identity(
    candidate: EvidenceCandidate,
    catalog: &PreparationCatalog,
    root: &Path,
    password: Option<&str>,
) -> String {
    let mut candidates = [candidate];
    annotate(
        &mut candidates,
        catalog,
        password,
        &AtomicBool::new(false),
        &mut |dependency, _| {
            fs::File::open(root.join(&dependency.file_id.0))
                .map(|file| CandidateSource::Reader(Box::new(file)))
                .map_err(|error| error.to_string())
        },
    )
    .unwrap();
    candidates[0].content_identity.clone()
}

#[test]
fn changed_timezone_bytes_invalidate_unchanged_log_metadata() {
    let root = workspace();
    fs::write(root.path().join("timezone"), b"UTC\n").unwrap();
    let mut catalog = PreparationCatalog::default();
    catalog
        .observe(&record("timezone", "/etc/timezone", 1))
        .unwrap();
    let input = candidate("LinuxArtifacts", "/var/log/dpkg.log", 1);
    let before = identity(input.clone(), &catalog, root.path(), None);
    fs::write(root.path().join("timezone"), b"GMT\n").unwrap();
    let after = identity(input, &catalog, root.path(), None);
    assert_ne!(before, after);
}

#[test]
fn dependency_identity_stays_stable_for_identical_inputs() {
    let root = workspace();
    fs::write(root.path().join("timezone"), b"UTC\n").unwrap();
    let mut catalog = PreparationCatalog::default();
    catalog
        .observe(&record("timezone", "/etc/timezone", 1))
        .unwrap();
    let input = candidate("LinuxArtifacts", "/var/log/dpkg.log", 1);
    assert_eq!(
        identity(input.clone(), &catalog, root.path(), None),
        identity(input, &catalog, root.path(), None)
    );
}

#[test]
fn another_partition_does_not_change_dependency_identity() {
    let root = workspace();
    fs::write(root.path().join("timezone-one"), b"UTC\n").unwrap();
    fs::write(root.path().join("timezone-two"), b"GMT\n").unwrap();
    let mut catalog = PreparationCatalog::default();
    catalog
        .observe(&record("timezone-one", "[P1]/etc/timezone", 1))
        .unwrap();
    catalog
        .observe(&record("timezone-two", "[P2]/etc/timezone", 2))
        .unwrap();
    let input = candidate("LinuxArtifacts", "[P1]/var/log/dpkg.log", 1);
    let before = identity(input.clone(), &catalog, root.path(), None);
    fs::write(root.path().join("timezone-two"), b"PST\n").unwrap();
    assert_eq!(before, identity(input, &catalog, root.path(), None));
}

#[test]
fn unavailable_dependency_is_retried_on_next_run() {
    let root = workspace();
    let mut catalog = PreparationCatalog::default();
    catalog
        .observe(&record("timezone", "/etc/timezone", 1))
        .unwrap();
    let input = candidate("LinuxArtifacts", "/var/log/dpkg.log", 1);
    assert_ne!(
        identity(input.clone(), &catalog, root.path(), None),
        identity(input, &catalog, root.path(), None)
    );
}

#[test]
fn user_secret_requires_fresh_browser_extraction() {
    let root = workspace();
    let catalog = PreparationCatalog::default();
    let input = candidate(
        "BrowserHistory",
        "/Users/alice/Google/Chrome/User Data/Default/Login Data",
        1,
    );
    let before = identity(input.clone(), &catalog, root.path(), Some("secret"));
    let after = identity(input, &catalog, root.path(), Some("secret"));
    assert_ne!(before, after);
    assert!(!before.contains("secret"));
}
