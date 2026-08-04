use std::io;
use std::sync::Arc;

use domain::DataSourceId;
use evidence_mount::{
    DirectoryPage, MountAccess, MountError, MountFileHandle, MountFileSystem, MountNode, MountPath,
    MountPlan, MountReadPolicy, MountSession,
};

struct MemoryFile(Vec<u8>);

impl MountFileHandle for MemoryFile {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::InvalidInput)?;
        Ok(self
            .0
            .get(start..start.saturating_add(length))
            .unwrap_or(&[])
            .to_vec())
    }
}

struct MemoryFs;

impl MountFileSystem for MemoryFs {
    fn lookup(&self, path: &MountPath) -> Result<MountNode, MountError> {
        if path.is_root() {
            return Ok(node(path, true, 0, "root"));
        }
        match path.as_str() {
            "/docs" => Ok(node(path, true, 0, "docs")),
            "/docs/readme.txt" => Ok(node(path, false, 5, "readme.txt")),
            _ => Err(MountError::NotFound(path.to_string())),
        }
    }

    fn read_directory(
        &self,
        path: &MountPath,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<DirectoryPage, MountError> {
        if !path.is_root() {
            return Err(MountError::NotDirectory(path.to_string()));
        }
        let start = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| MountError::InvalidCursor)?;
        let entries = [
            node(&MountPath::parse("/docs")?, true, 0, "docs"),
            node(
                &MountPath::parse("/docs/readme.txt")?,
                false,
                5,
                "readme.txt",
            ),
        ];
        let end = (start + limit as usize).min(entries.len());
        Ok(DirectoryPage {
            entries: entries[start..end].to_vec(),
            next_cursor: (end < entries.len()).then(|| end.to_string()),
        })
    }

    fn open_read(&self, path: &MountPath) -> Result<Box<dyn MountFileHandle>, MountError> {
        self.lookup(path)?;
        Ok(Box::new(MemoryFile(b"hello".to_vec())))
    }
}

fn node(path: &MountPath, is_dir: bool, size: u64, name: &str) -> MountNode {
    MountNode {
        path: path.clone(),
        name: name.to_string(),
        is_dir,
        size,
        read_only: true,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        source_file_id: None,
    }
}

fn session(policy: MountReadPolicy) -> MountSession {
    let plan = MountPlan::new(
        DataSourceId("source-1".to_string()),
        0,
        "NTFS",
        "sha256:test",
    )
    .expect("valid plan");
    MountSession::new(plan, Arc::new(MemoryFs), policy)
}

#[test]
fn normalizes_virtual_paths_and_rejects_traversal() {
    assert_eq!(
        MountPath::parse(r"\\docs\\readme.txt").unwrap().as_str(),
        "/docs/readme.txt"
    );
    assert_eq!(MountPath::parse("/").unwrap(), MountPath::root());
    assert_eq!(
        MountPath::parse("/docs/../secret"),
        Err(MountError::PathTraversal)
    );
    assert!(matches!(
        MountPath::parse("/docs/readme.txt:stream"),
        Err(MountError::InvalidPath(_))
    ));
    assert!(matches!(
        MountPath::parse("/CON"),
        Err(MountError::InvalidPath(_))
    ));
}

#[test]
fn rejects_write_access_and_bounds_reads() {
    let session = session(MountReadPolicy {
        max_read_length: 3,
        ..MountReadPolicy::default()
    });
    let path = MountPath::parse("/docs/readme.txt").unwrap();
    assert_eq!(
        session.open(&path, MountAccess::Write),
        Err(MountError::WriteDenied)
    );
    let handle = session.open(&path, MountAccess::ReadOnly).unwrap();
    assert_eq!(
        session.read_at(handle, 1, 4),
        Err(MountError::ReadLimit {
            requested: 4,
            maximum: 3
        })
    );
    assert_eq!(
        session.read_at(handle, 6, 1),
        Err(MountError::OffsetOutOfBounds { offset: 6, size: 5 })
    );
    assert_eq!(session.read_at(handle, 3, 3).unwrap(), b"lo".to_vec());
    session.close(handle).unwrap();
}

#[test]
fn enforces_handle_limit_and_directory_cursor() {
    let session = session(MountReadPolicy {
        max_open_handles: 1,
        ..MountReadPolicy::default()
    });
    let path = MountPath::parse("/docs/readme.txt").unwrap();
    let first = session.open(&path, MountAccess::ReadOnly).unwrap();
    assert_eq!(
        session.open(&path, MountAccess::ReadOnly),
        Err(MountError::HandleLimit)
    );
    session.close(first).unwrap();

    let page = session.read_directory(&MountPath::root(), None, 1).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.next_cursor.as_deref(), Some("1"));
    let second = session
        .read_directory(&MountPath::root(), page.next_cursor.as_deref(), 1)
        .unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.next_cursor, None);
    assert_eq!(
        session.read_directory(&MountPath::root(), None, 0),
        Err(MountError::InvalidDirectoryLimit)
    );
}
