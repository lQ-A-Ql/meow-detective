use app_services::analysis_service::{
    discover_evidence_candidates, evidence_candidates_for_categories,
};

#[path = "support/native_analysis_fixture.rs"]
mod native_analysis_fixture;

use native_analysis_fixture::{source_file, NativeAnalysisFixture};

const SOURCE_ID: &str = "source-1";

fn candidate_db() -> NativeAnalysisFixture {
    NativeAnalysisFixture::new(SOURCE_ID, "case-candidates")
}

fn insert_file(fixture: &NativeAnalysisFixture, id: &str, path: &str) {
    fixture.insert_file(source_file(SOURCE_ID, id, path, 1, false));
}

fn insert_partitioned_file(
    fixture: &NativeAnalysisFixture,
    id: &str,
    path: &str,
    encrypted: bool,
    partition_index: usize,
) {
    fixture.insert_partitioned_file(
        source_file(SOURCE_ID, id, path, 12, encrypted),
        partition_index,
    );
}

#[test]
fn email_candidates_keep_extension_specific_kind_and_parser() {
    let connection = candidate_db();
    let expected = [
        ("eml", "mail.eml", "email_eml_emlx", "email.eml_emlx"),
        ("emlx", "mail.emlx", "email_eml_emlx", "email.eml_emlx"),
        ("mbox", "inbox.mbox", "email_mbox", "email.mbox"),
        ("pst", "archive.pst", "email_pst", "email.pst"),
        ("ost", "cache.ost", "email_ost", "email.ost"),
    ];
    for &(id, path, _, _) in &expected {
        insert_file(&connection, id, path);
    }
    insert_file(&connection, "other", "unknown.bin");

    let discovered = discover_evidence_candidates(&connection).expect("discover email evidence");
    let email = discovered.get("Email").expect("email category");
    assert_eq!(email.len(), expected.len());
    for &(id, _, evidence_kind, parser) in &expected {
        let candidate = email
            .iter()
            .find(|candidate| candidate.file_id.0 == id)
            .expect("extension-specific email candidate");
        assert_eq!(candidate.evidence_kind.as_str(), evidence_kind);
        assert_eq!(candidate.parser.as_str(), parser);
    }
}

#[test]
fn linux_candidate_discovery_strips_partition_and_lvm_root_prefixes() {
    let connection = candidate_db();
    let paths = [
        "Partition 2 (XFS) - cl/root/etc/passwd",
        "[P2]/cl/root/var/log/auth.log.1.gz",
        "cl/root/home/alice/.bash_history",
        "cl/root/root/.bash_history",
    ];
    for (index, path) in paths.iter().enumerate() {
        insert_file(&connection, &format!("linux-{index}"), path);
    }

    let discovered =
        discover_evidence_candidates(&connection).expect("discover normalized Linux evidence");
    let linux = discovered
        .get("LinuxArtifacts")
        .expect("Linux artifact category");
    assert_eq!(linux.len(), paths.len());
    assert!(paths.iter().all(|path| linux
        .iter()
        .any(|candidate| candidate.path.as_str() == *path)));
}

#[test]
fn linux_only_discovery_returns_no_windows_candidates() {
    let connection = candidate_db();
    insert_file(&connection, "linux-auth", "var/log/auth.log");
    insert_file(
        &connection,
        "windows-registry",
        "Windows/System32/config/SYSTEM",
    );
    insert_file(
        &connection,
        "windows-event",
        "Windows/System32/winevt/Logs/System.evtx",
    );

    let candidates = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover Linux-only evidence");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].file_id.0, "linux-auth");
    assert_eq!(candidates[0].category, "LinuxArtifacts");
}

#[test]
fn discovery_carries_file_entry_partition_index() {
    let connection = candidate_db();
    insert_partitioned_file(&connection, "linux-web", "var/www/html/index.php", false, 7);

    let candidates = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover partition-bound candidate");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].partition_index, Some(7));
}

#[test]
fn discovery_preserves_efs_encryption_fact_and_identity() {
    let connection = candidate_db();
    insert_partitioned_file(
        &connection,
        "encrypted-web",
        "var/www/html/secret.php",
        true,
        7,
    );

    let encrypted = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover encrypted candidate")
        .remove(0);
    assert!(encrypted.encrypted);

    connection.set_encryption("encrypted-web", None);
    let unknown = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover unknown-status candidate")
        .remove(0);
    assert!(unknown.encrypted, "unknown candidate must fail closed");
    assert_ne!(encrypted.content_identity, unknown.content_identity);

    connection.set_encryption("encrypted-web", Some(false));
    let unencrypted = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("rediscover unencrypted candidate")
        .remove(0);

    assert!(!unencrypted.encrypted);
    assert_ne!(encrypted.content_identity, unencrypted.content_identity);
    assert_ne!(unknown.content_identity, unencrypted.content_identity);
}

#[test]
fn native_partition_index_rejects_value_above_supported_range() {
    let connection = candidate_db();
    connection.reject_partition_index_above_u32(source_file(
        SOURCE_ID,
        "oversized",
        "var/www/oversized.php",
        12,
        false,
    ));
}
