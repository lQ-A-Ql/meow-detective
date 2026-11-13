use domain::{CaseId, CaseMeta};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    evidence_digest_repo::{EvidenceDigestRecord, EvidenceDigestRepo},
};

#[test]
fn file_sha256_lookup_is_bounded_and_scoped_to_current_completed_file_records() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    let now = chrono::Utc::now();
    for id in ["one", "two"] {
        CaseRepo::new(&connection)
            .create(&CaseMeta {
                id: CaseId(id.into()),
                name: id.into(),
                number: None,
                examiner: None,
                notes: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap();
    }
    let repo = EvidenceDigestRepo::new(&connection);
    let base = EvidenceDigestRecord {
        case_id: "one".into(),
        scope: "File".into(),
        algorithm: "sha256".into(),
        data_source_id: Some("source".into()),
        file_id: Some("file".into()),
        partition_index: None,
        value: "a".repeat(64),
        byte_length: 3,
        status: "completed".into(),
        source_revision: "current".into(),
    };
    repo.upsert(&base).unwrap();
    for record in [
        EvidenceDigestRecord {
            case_id: "two".into(),
            ..base.clone()
        },
        EvidenceDigestRecord {
            data_source_id: Some("foreign".into()),
            ..base.clone()
        },
        EvidenceDigestRecord {
            algorithm: "md5".into(),
            ..base.clone()
        },
        EvidenceDigestRecord {
            partition_index: Some(1),
            ..base.clone()
        },
        EvidenceDigestRecord {
            file_id: Some("failed".into()),
            status: "failed".into(),
            ..base.clone()
        },
    ] {
        repo.upsert(&record).unwrap();
    }
    let values = repo
        .file_sha256_values("one", "source", &["file", "failed"])
        .unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].file_id, "file");
    assert_eq!(values[0].value, base.value);
    assert_eq!(values[0].source_revision, "current");
    assert!(repo
        .file_sha256_values("one", "source", &[])
        .unwrap()
        .is_empty());
    assert!(repo
        .file_sha256_values("one", "source", &vec!["file"; 501])
        .is_err());
}
