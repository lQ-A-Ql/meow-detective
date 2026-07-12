use super::*;

fn make_file(name: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId("test".to_string()),
        parent_id: Some(FileEntryId("parent".to_string())),
        data_source_id: crate::DataSourceId("ds".to_string()),
        path: format!("/test/{}", name),
        name: name.to_string(),
        entry_type: EntryType::File,
        size: Some(1024),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn make_dir(name: &str) -> FileEntry {
    let mut entry = make_file(name);
    entry.entry_type = EntryType::Directory;
    entry.size = None;
    entry
}

#[test]
fn is_file_true() {
    assert!(make_file("test.txt").is_file());
}

#[test]
fn is_file_false() {
    assert!(!make_dir("docs").is_file());
}

#[test]
fn is_directory_true() {
    assert!(make_dir("docs").is_directory());
}

#[test]
fn is_hidden_true() {
    assert!(make_file(".gitignore").is_hidden());
}

#[test]
fn is_hidden_false() {
    assert!(!make_file("readme.txt").is_hidden());
}

#[test]
fn extension_basic() {
    assert_eq!(make_file("test.txt").extension(), Some("txt"));
    assert_eq!(make_file("archive.tar.gz").extension(), Some("gz"));
}

#[test]
fn extension_none() {
    assert_eq!(make_file("Makefile").extension(), None);
    // .gitignore has no extension (dot is first char, so rsplit returns "gitignore")
    // but our implementation filters out cases where the dot is the first char
    // because rsplit('.').next() on ".gitignore" returns "gitignore" which equals the stem
    assert_eq!(make_file(".gitignore").extension(), None);
}

#[test]
fn size_or_zero_file() {
    assert_eq!(make_file("test.txt").size_or_zero(), 1024);
}

#[test]
fn size_or_zero_directory() {
    assert_eq!(make_dir("docs").size_or_zero(), 0);
}

#[test]
fn is_root_true() {
    let mut entry = make_file("root.txt");
    entry.parent_id = None;
    assert!(entry.is_root());
}

#[test]
fn is_root_false() {
    assert!(!make_file("child.txt").is_root());
}

#[test]
fn is_deleted() {
    let mut entry = make_file("deleted.txt");
    entry.deleted = true;
    assert!(entry.is_deleted());
}

#[test]
fn has_hash_true() {
    let mut entry = make_file("test.txt");
    entry.hash_sha256 = Some("abc123".to_string());
    assert!(entry.has_hash());
}

#[test]
fn has_hash_false() {
    assert!(!make_file("test.txt").has_hash());
}
