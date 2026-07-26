use app_services::analysis_service::{
    discover_evidence_candidates, evidence_candidates_for_categories, AnalysisServiceError,
};
use rusqlite::{params, Connection};

fn candidate_db() -> Connection {
    let connection = Connection::open_in_memory().expect("open in-memory candidate database");
    connection
        .execute_batch(
            "CREATE TABLE file_entries (
                id TEXT PRIMARY KEY,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                size INTEGER,
                partition_index INTEGER,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT,
                encrypted INTEGER CHECK (encrypted IS NULL OR encrypted IN (0, 1)),
                entry_type TEXT NOT NULL
            );",
        )
        .expect("create candidate schema");
    connection
}

fn insert_file(connection: &Connection, id: &str, path: &str) {
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, encrypted, entry_type)
             VALUES (?1, 'source-1', ?2, 1, 0, 'file')",
            params![id, path],
        )
        .expect("insert candidate file");
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
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, partition_index, entry_type)
             VALUES ('linux-web', 'source-1', 'var/www/html/index.php', 12, 7, 'file')",
            [],
        )
        .expect("insert partition-bound candidate");

    let candidates = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover partition-bound candidate");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].partition_index, Some(7));
}

#[test]
fn discovery_preserves_efs_encryption_fact_and_identity() {
    let connection = candidate_db();
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, partition_index, encrypted, entry_type)
             VALUES ('encrypted-web', 'source-1', 'var/www/html/secret.php', 12, 7, 1, 'file')",
            [],
        )
        .expect("insert encrypted candidate");

    let encrypted = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover encrypted candidate")
        .remove(0);
    assert!(encrypted.encrypted);

    connection
        .execute(
            "UPDATE file_entries SET encrypted = NULL WHERE id = 'encrypted-web'",
            [],
        )
        .expect("mark encryption status unknown");
    let unknown = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("discover unknown-status candidate")
        .remove(0);
    assert!(unknown.encrypted, "unknown candidate must fail closed");
    assert_ne!(encrypted.content_identity, unknown.content_identity);

    connection
        .execute(
            "UPDATE file_entries SET encrypted = 0 WHERE id = 'encrypted-web'",
            [],
        )
        .expect("clear encryption flag");
    let unencrypted = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect("rediscover unencrypted candidate")
        .remove(0);

    assert!(!unencrypted.encrypted);
    assert_ne!(encrypted.content_identity, unencrypted.content_identity);
    assert_ne!(unknown.content_identity, unencrypted.content_identity);
}

#[test]
fn discovery_rejects_negative_partition_index() {
    let connection = candidate_db();
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, partition_index, entry_type)
             VALUES ('negative', 'source-1', 'var/www/negative.php', 12, -1, 'file')",
            [],
        )
        .expect("insert negative partition index");

    let error = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect_err("negative partition index must fail");

    assert!(matches!(error, AnalysisServiceError::InvalidInput(_)));
    assert!(error.to_string().contains("partition_index -1"));
}

#[test]
fn discovery_rejects_partition_index_above_supported_range() {
    let connection = candidate_db();
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, partition_index, entry_type)
             VALUES ('oversized', 'source-1', 'var/www/oversized.php', 12, 4294967296, 'file')",
            [],
        )
        .expect("insert oversized partition index");

    let error = evidence_candidates_for_categories(&connection, &["LinuxArtifacts"])
        .expect_err("oversized partition index must fail");

    assert!(matches!(error, AnalysisServiceError::InvalidInput(_)));
    assert!(error.to_string().contains("partition_index 4294967296"));
}
