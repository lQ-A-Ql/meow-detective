use std::io::{self, Cursor, Read};

use evidence_core::{FileSystemReader, FsNode};

use super::*;

#[derive(Default)]
struct MockFilesystem;

impl FileSystemReader for MockFilesystem {
    fn root(&self) -> io::Result<FsNode> {
        Ok(evidence_core::filesystem::root_node())
    }

    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Ok(Vec::new())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "mock"
    }
}

fn runtime_key(source: &str) -> RuntimeKey {
    RuntimeKey {
        case_id: "case-a".to_string(),
        data_source_id: source.to_string(),
    }
}

fn filesystem_key(source: &str, candidate: &str) -> FilesystemKey {
    FilesystemKey {
        runtime: runtime_key(source),
        fingerprint: "a".repeat(64),
        candidate_identity: candidate.to_string(),
    }
}

fn shared_filesystem() -> SharedPreparedFilesystem {
    Arc::new(Mutex::new(Box::new(MockFilesystem)))
}

#[test]
fn matching_filesystem_reuses_the_same_reader_instance() {
    let mut state = RegistryState::default();
    let key = filesystem_key("source-a", "partition-1");
    let filesystem = shared_filesystem();
    let expected = filesystem.clone();
    insert_filesystem_locked(&mut state, key.clone(), filesystem, 8);

    let resolved = matching_filesystem(&mut state, &key).expect("cached filesystem");

    assert!(Arc::ptr_eq(&resolved, &expected));
    assert_eq!(state.filesystems.len(), 1);
}

#[test]
fn runtime_invalidation_removes_all_partition_filesystems() {
    let mut state = RegistryState::default();
    let runtime = runtime_key("source-a");
    for candidate in ["partition-1", "partition-2"] {
        insert_filesystem_locked(
            &mut state,
            filesystem_key("source-a", candidate),
            shared_filesystem(),
            8,
        );
    }
    insert_filesystem_locked(
        &mut state,
        filesystem_key("source-b", "partition-1"),
        shared_filesystem(),
        8,
    );

    invalidate_filesystems_for_runtime(&mut state, &runtime);

    assert_eq!(state.filesystems.len(), 1);
    assert!(state
        .filesystems
        .keys()
        .all(|key| key.runtime.data_source_id == "source-b"));
}

#[test]
fn filesystem_budget_does_not_evict_an_active_session_reader() {
    let mut state = RegistryState::default();
    let first_key = filesystem_key("source-a", "partition-1");
    let first = shared_filesystem();
    let active_session = first.clone();
    insert_filesystem_locked(&mut state, first_key.clone(), first, 1);
    insert_filesystem_locked(
        &mut state,
        filesystem_key("source-a", "partition-2"),
        shared_filesystem(),
        1,
    );

    assert!(state.filesystems.contains_key(&first_key));
    assert_eq!(state.filesystems.len(), 1);
    drop(active_session);
}
