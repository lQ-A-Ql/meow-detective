use super::*;

fn setup() -> rusqlite::Connection {
    let connection = crate::connection::open_in_memory().unwrap();
    crate::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases(id,name,number,examiner) VALUES ('case-1','Case','1','qa')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO jobs(id,case_id,kind,status,progress,detail)
             VALUES ('job-1','case-1','contentIndex','pending',0,'')",
            [],
        )
        .unwrap();
    connection
}

#[test]
fn binding_is_idempotent_and_ownership_is_case_session_scoped() {
    let connection = setup();
    let repo = McpTaskBindingRepo::new(&connection);
    let binding = McpTaskBinding {
        case_id: "case-1".into(),
        case_session_id: "session-1".into(),
        principal_id: "client-1".into(),
        operation: "start_content_index".into(),
        idempotency_key: "request-1".into(),
        arguments_sha256: "hash-1".into(),
        task_id: "job-1".into(),
    };

    assert_eq!(repo.insert_or_get(&binding).unwrap(), binding);
    assert_eq!(repo.insert_or_get(&binding).unwrap(), binding);
    assert!(repo
        .find_owned_task("case-1", "session-1", "client-1", "job-1")
        .unwrap());
    assert!(!repo
        .find_owned_task("case-1", "session-2", "client-1", "job-1")
        .unwrap());
}
