use super::*;
use crate::filesystem::FileSystemReader;

#[cfg(unix)]
#[test]
fn list_children_does_not_descend_into_symlinked_directories() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("outside.txt"), b"outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("linked")).unwrap();

    let reader = LogicalFsReader::open(tmp.path(), "fixture").unwrap();
    let children = reader.list_children("").unwrap();
    let linked = children
        .iter()
        .find(|child| child.name == "linked")
        .unwrap();

    assert!(!linked.is_dir);
}

#[cfg(windows)]
#[test]
fn list_children_does_not_descend_into_symlinked_directories() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("outside.txt"), b"outside").unwrap();
    match std::os::windows::fs::symlink_dir(outside.path(), tmp.path().join("linked")) {
        Ok(()) => {}
        Err(err)
            if err.kind() == std::io::ErrorKind::PermissionDenied
                || err.raw_os_error() == Some(1314) =>
        {
            return;
        }
        Err(err) => panic!("failed to create symlink fixture: {err}"),
    }

    let reader = LogicalFsReader::open(tmp.path(), "fixture").unwrap();
    let children = reader.list_children("").unwrap();
    let linked = children
        .iter()
        .find(|child| child.name == "linked")
        .unwrap();

    assert!(!linked.is_dir);
}
