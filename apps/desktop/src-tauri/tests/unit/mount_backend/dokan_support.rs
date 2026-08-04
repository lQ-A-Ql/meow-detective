use std::sync::Arc;

use domain::DataSourceId;
use evidence_mount::{
    DirectoryPage, MountError, MountFileHandle, MountFileSystem, MountNode, MountPath, MountPlan,
    MountReadPolicy, MountSession,
};
use widestring::U16CString;

use super::{find_directory_files, has_write_access, windows_filesystem_name};
use winapi::um::winnt;

struct LargeDirectoryFilesystem;

impl MountFileSystem for LargeDirectoryFilesystem {
    fn lookup(&self, path: &MountPath) -> Result<MountNode, MountError> {
        if !path.is_root() {
            return Err(MountError::NotFound(path.to_string()));
        }
        Ok(directory_node(path.clone(), "root"))
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
        let offset = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| MountError::InvalidCursor)?;
        let end = offset.saturating_add(limit as usize).min(600);
        let entries = (offset..end)
            .map(|index| {
                let name = format!("file-{index:04}.txt");
                MountNode {
                    path: MountPath::parse(&name).expect("valid generated path"),
                    name,
                    is_dir: false,
                    size: 1,
                    read_only: true,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    source_file_id: None,
                }
            })
            .collect();
        Ok(DirectoryPage {
            entries,
            next_cursor: (end < 600).then(|| end.to_string()),
        })
    }

    fn open_read(&self, _path: &MountPath) -> Result<Box<dyn MountFileHandle>, MountError> {
        Err(MountError::Filesystem("not used".to_string()))
    }
}

fn directory_node(path: MountPath, name: &str) -> MountNode {
    MountNode {
        path,
        name: name.to_string(),
        is_dir: true,
        size: 0,
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

fn large_directory_session() -> MountSession {
    let plan = MountPlan::new(DataSourceId("source".to_string()), 1, "NTFS", "sha256:test")
        .expect("valid mount plan");
    MountSession::new(
        plan,
        Arc::new(LargeDirectoryFilesystem),
        MountReadPolicy::default(),
    )
}

#[test]
fn create_access_mask_rejects_every_write_capability() {
    assert!(!has_write_access(winnt::GENERIC_READ));
    for access in [
        winnt::GENERIC_WRITE,
        winnt::GENERIC_ALL,
        winnt::FILE_WRITE_DATA,
        winnt::FILE_APPEND_DATA,
        winnt::FILE_WRITE_EA,
        winnt::FILE_WRITE_ATTRIBUTES,
        winnt::FILE_DELETE_CHILD,
        winnt::WRITE_DAC,
        winnt::WRITE_OWNER,
        winnt::DELETE,
    ] {
        assert!(has_write_access(access), "write access mask {access:#x}");
    }
}

#[test]
fn windows_filesystem_name_uses_known_windows_compatibility_names() {
    assert_eq!(
        windows_filesystem_name("NTFS")
            .expect("static NTFS name must be valid")
            .to_string_lossy(),
        "NTFS"
    );
    assert_eq!(
        windows_filesystem_name("FAT32")
            .expect("static FAT name must be valid")
            .to_string_lossy(),
        "FAT"
    );
    assert_eq!(
        windows_filesystem_name("exFAT")
            .expect("static exFAT name must be valid")
            .to_string_lossy(),
        "exFAT"
    );
    assert_eq!(
        windows_filesystem_name("XFS")
            .expect("static fallback name must be valid")
            .to_string_lossy(),
        "NTFS"
    );
}

#[test]
fn find_files_emits_the_complete_directory_in_one_callback() {
    let session = large_directory_session();
    let root = U16CString::from_str("\\").expect("root path");
    let mut emitted = 0usize;

    find_directory_files(&session, root.as_ucstr(), &mut |_| {
        emitted = emitted.saturating_add(1);
        Ok(())
    })
    .expect("directory enumeration");

    assert_eq!(emitted, 600);
}
