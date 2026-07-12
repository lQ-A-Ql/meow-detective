use super::*;

#[test]
fn handle_create_get_delete() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = HandleRepo::new(&conn);

    let handle_id = repo
        .create("case-1", "obj-1", Duration::minutes(30))
        .unwrap();
    assert!(!handle_id.is_empty());

    let handle = repo.get(&handle_id).unwrap().unwrap();
    assert_eq!(handle.case_id, "case-1");
    assert_eq!(handle.object_id, "obj-1");
    assert_eq!(handle.access_mode, "read");

    repo.delete(&handle_id).unwrap();
    assert!(repo.get(&handle_id).unwrap().is_none());
}

#[test]
fn handle_expired() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = HandleRepo::new(&conn);

    let handle_id = repo
        .create("case-1", "obj-1", Duration::seconds(-1))
        .unwrap();
    assert!(repo.get(&handle_id).unwrap().is_none());
}

#[test]
fn handle_cleanup() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = HandleRepo::new(&conn);

    repo.create("case-1", "obj-1", Duration::seconds(-1))
        .unwrap();
    repo.create("case-1", "obj-2", Duration::seconds(-1))
        .unwrap();
    repo.create("case-1", "obj-3", Duration::minutes(30))
        .unwrap();

    let cleaned = repo.cleanup_expired().unwrap();
    assert_eq!(cleaned, 2);
}
