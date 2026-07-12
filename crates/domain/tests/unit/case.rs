use super::*;

fn make_test_case() -> CaseMeta {
    CaseMeta {
        id: CaseId("test-123".to_string()),
        name: "Test Case".to_string(),
        number: Some("2024-001".to_string()),
        examiner: Some("John Doe".to_string()),
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn display_name_with_number() {
    let case = make_test_case();
    assert_eq!(case.display_name(), "[2024-001] Test Case");
}

#[test]
fn display_name_without_number() {
    let mut case = make_test_case();
    case.number = None;
    assert_eq!(case.display_name(), "Test Case");
}

#[test]
fn is_active_recent() {
    let case = make_test_case();
    assert!(case.is_active());
}

#[test]
fn is_active_old() {
    let mut case = make_test_case();
    case.updated_at = Utc::now() - chrono::Duration::hours(48);
    assert!(!case.is_active());
}

#[test]
fn has_examiner_true() {
    let case = make_test_case();
    assert!(case.has_examiner());
}

#[test]
fn has_examiner_empty() {
    let mut case = make_test_case();
    case.examiner = Some("".to_string());
    assert!(!case.has_examiner());
}

#[test]
fn has_examiner_none() {
    let mut case = make_test_case();
    case.examiner = None;
    assert!(!case.has_examiner());
}

#[test]
fn summary_with_examiner() {
    let case = make_test_case();
    assert_eq!(case.summary(), "Test Case by John Doe");
}

#[test]
fn summary_without_examiner() {
    let mut case = make_test_case();
    case.examiner = None;
    assert_eq!(case.summary(), "Test Case");
}

#[test]
fn session_db_path() {
    let session = CaseSession {
        case_id: CaseId("test".to_string()),
        case_root: PathBuf::from("/tmp/test-case"),
        opened_at: Utc::now(),
    };
    assert_eq!(
        session.db_path(),
        PathBuf::from("/tmp/test-case/forensics.db")
    );
}

#[test]
fn session_indexes_path() {
    let session = CaseSession {
        case_id: CaseId("test".to_string()),
        case_root: PathBuf::from("/tmp/test-case"),
        opened_at: Utc::now(),
    };
    assert_eq!(
        session.indexes_path(),
        PathBuf::from("/tmp/test-case/indexes")
    );
}
