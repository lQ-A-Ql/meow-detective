use evidence_core::filesystem::logical_fs::LogicalFsReader;
use evidence_core::filesystem::FileSystemReader;
use std::io::Read;
use tempfile::TempDir;

#[test]
fn enumerate_root() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
    std::fs::write(tmp.path().join("b.txt"), b"world").unwrap();
    std::fs::create_dir(tmp.path().join("subdir")).unwrap();

    let fs = LogicalFsReader::open(tmp.path(), "test-fs").unwrap();
    let root = fs.root().unwrap();
    assert!(root.is_dir);

    let children = fs.list_children("").unwrap();
    assert_eq!(children.len(), 3);
    let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"b.txt"));
    assert!(names.contains(&"subdir"));
}

#[test]
fn list_children_of_subdir() {
    let tmp = TempDir::new().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("inner.txt"), b"data").unwrap();

    let fs = LogicalFsReader::open(tmp.path(), "test").unwrap();
    let children = fs.list_children("sub").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "inner.txt");
    assert_eq!(children[0].path, "sub/inner.txt");
}

#[test]
fn stat_returns_metadata() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("meta.txt"), b"test data").unwrap();

    let fs = LogicalFsReader::open(tmp.path(), "test").unwrap();
    let children = fs.list_children("").unwrap();
    let file = children.iter().find(|c| c.name == "meta.txt").unwrap();
    assert!(!file.is_dir);
    assert_eq!(file.size, 9);
}

#[test]
fn open_file_reads_content() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("data.txt"), b"forensics test content").unwrap();

    let fs = LogicalFsReader::open(tmp.path(), "test").unwrap();
    let mut reader = fs.open_file("data.txt").unwrap();
    let mut content = String::new();
    reader.read_to_string(&mut content).unwrap();
    assert_eq!(content, "forensics test content");
}

#[test]
fn directories_sorted_first() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("zzz_dir")).unwrap();
    std::fs::write(tmp.path().join("aaa_file.txt"), b"x").unwrap();

    let fs = LogicalFsReader::open(tmp.path(), "test").unwrap();
    let children = fs.list_children("").unwrap();
    assert!(children[0].is_dir);
    assert_eq!(children[0].name, "zzz_dir");
    assert!(!children[1].is_dir);
}
